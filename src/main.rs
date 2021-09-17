use std::{borrow::BorrowMut, convert::TryInto, env, error::Error, fmt::{self, Formatter, Display}, fs::File, io::prelude::*, net::Ipv4Addr, process::exit, sync::Arc};
use futures::lock::Mutex;
use tini::Ini;
use native_tls::Identity;
use tokio::{io::{AsyncReadExt, AsyncWriteExt}, net::TcpStream};
use tokio::net::TcpListener;
use tokio_native_tls::TlsStream;
use self::request::Request;
use self::response::Response;
use self::api::API;
use log::info;
use log::error;
use std::time::Duration;
use deadpool_postgres::{Config, ManagerConfig, Pool, RecyclingMethod };
use tokio_postgres::NoTls;

mod db;
mod request;
mod response;
mod api;

#[macro_use]
extern crate serde;
extern crate log;
extern crate serde_json;

//#[json]
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RequestMethod{
    GET,
    POST,
    PATCH,
    DELETE,
    POSTorPATCHasGET,
    SHUTDOWN,
    RELOAD,
    UNKNOWN
}

pub enum ParameterType{
    STRING,
    INTEGER,
    BIGINT,
    BOOLEAN,
    NUMBER,
    UNKNOWN
}
    
impl ParameterType{
    pub fn from( s_name: &str ) -> Self{
        match &s_name.to_ascii_lowercase()[..]{
            "string" => ParameterType::STRING,
            "integer" => ParameterType::INTEGER,
            "boolean" => ParameterType::BOOLEAN,
            "bigint" => ParameterType::BIGINT,
            "number" => ParameterType::NUMBER,
            _ => ParameterType::UNKNOWN,
        }
    }
}

impl Default for RequestMethod {
    fn default() -> Self { RequestMethod::UNKNOWN }
}

impl Display for RequestMethod {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self{
            RequestMethod::GET => write!(f, "Http GET"),
            RequestMethod::POST => write!(f, "Http POST"),
            RequestMethod::PATCH => write!(f, "Http PATCH"),
            RequestMethod::DELETE => write!(f, "Http DELETE"),
            RequestMethod::POSTorPATCHasGET => write!(f, "Http POST -> GET"),
            RequestMethod::SHUTDOWN => write!(f, "SHUTDOWN"),
            _ => write!(f, "Unknown")
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Authentication{
    NEEDED,
    NOTNEEDED,
    UNKNOWN
}

impl Default for Authentication {
    fn default() -> Self { Authentication::UNKNOWN }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ParamVal {
    Int(i32),
    BigInt(i64),
    Float(f64),
    Text(String),
    Date(String),
    Boolean(bool),
}

// Helper so that http and https connections can
// be handled under one umbrella stream: "VarStream"
// Exposes read and write_all
// @TODO: isn't ´await` called twice on these methods? Check reference and experiment
pub enum VarStream{
   Secure( TlsStream<TcpStream> ),
   Insecure( TcpStream ), 
}

impl VarStream{
    async fn read(&mut self, buffer:&mut Vec<u8>) -> tokio::io::Result<usize>{
        match self{
            VarStream::Secure( d ) => d.read(buffer).await,
            VarStream::Insecure( ds ) => ds.read(buffer).await
        }
    }
    async fn write_all(&mut self, buffer:&mut Vec<u8>) -> tokio::io::Result<()>{
        match self{
            VarStream::Secure( d ) => d.write_all(buffer).await,
            VarStream::Insecure( ds ) => ds.write_all(buffer).await
        }
    }
}

const S_EMPTY: String = String::new();

// ----------------------------------------------------------------------------------------
// Structs

// ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
// OpenAPI JSON Format
#[derive(Serialize, Deserialize, Debug)]
struct Schema {
    r#type: String,
    format: String
}

#[derive(Serialize, Deserialize, Debug)]
pub struct APIParam {
    name: String,
    description: String,
    r#in: String,
    required: bool,
    schema: Schema
}

#[derive(Serialize, Deserialize, Debug)]
struct APIError {
    message: String,
    hint: String
}

#[derive(PartialEq,Serialize, Clone, Deserialize, Copy, Debug)]
pub enum CPRelation{
    Unknown,
    Equal,
    NotEqual,
    LessThan,
    GreaterThan,
    LessOrEqual,
    GreaterOrEqual,
    Like,
    In
}

impl CPRelation{
    fn db_rep( rel: &CPRelation ) -> String{
        match rel {
            CPRelation::Unknown => "".to_string(),
            CPRelation::Equal => "=".to_string(),
            CPRelation::NotEqual => "!=".to_string(),
            CPRelation::LessThan=> "<".to_string(),
            CPRelation::GreaterThan => ">".to_string(),
            CPRelation::LessOrEqual=> "<=".to_string(),
            CPRelation::GreaterOrEqual=> ">=".to_string(),
            CPRelation::Like => " LIKE ".to_string(),
            CPRelation::In => " IN ".to_string()
        }
    }

    pub fn new( s: &str ) -> Self{
        match s{
            "eq" => CPRelation::Equal,
            "ne" => CPRelation::NotEqual,
            "lt" => CPRelation::LessThan,
            "le" => CPRelation::LessOrEqual,
            "gt" => CPRelation::GreaterThan,
            "ge" => CPRelation::GreaterOrEqual,
            "like" => CPRelation::Like,
            "in" => CPRelation::In,
            _ => CPRelation::Unknown
        }
    }
}
impl fmt::Display for CPRelation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
      write!(f, "{}", CPRelation::db_rep( self) )
    }
}

// Adding Default because Clone for UnCheckedParam is not satisfied
impl Default for CPRelation {
    fn default() -> Self { CPRelation::Unknown }
}

// Adding Default because Clone for UnCheckedParam is not satisfied
impl Default for ParamVal {
    fn default() -> Self { ParamVal::Text( "not initialized".to_string() ) }
}

// ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
// UnCheckedParam are compared against
// API and transformed to CheckedParam if
// they contain no problems.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct UnCheckedParam{
    problem: String,
    name: String,
    relation: CPRelation,           // If 'extended syntax' is used, =5 must be handed over as =eq.5,
    use_extended_syntax: bool,      // and =lt.6 represents <6 for the database. Possible relations (=, < etc.)
    value: ParamVal                 // are represented in CPRelation 
}

#[derive(Debug, Clone)]
pub struct CheckedParam{
    name: String,
    relation: CPRelation,
    value: ParamVal
}

impl CheckedParam {
    pub fn new(name: String, value: ParamVal) -> Self { CheckedParam { name, relation: CPRelation::Equal, value } }
    pub fn new_ext(name: String, value: ParamVal, relation: CPRelation) -> Self { CheckedParam { name, relation, value } }
}

/**
 * UnCheckedParams represent both query parameters (which 
 * come by name and a String value) and payload parameters.
 * 
 * Payload parameters come in a Serde value (rather than a String
 * value)
 */
impl UnCheckedParam{ 
//    pub fn new(name: String, value: ParamVal, problem: String) -> Self { UnCheckedParam { problem, name, relation: CPRelation::Unknown, value } }
    pub fn new_query_parameter(name: &str, value: &str, expected_type: ParameterType) -> Self{
        let check = UnCheckedParam::get_typecheck_of_query_parameter( value, expected_type ); // .1 ist Problem, .0 ist value
        UnCheckedParam { problem: check.1, name: name.to_string(), relation: CPRelation::Unknown, value: check.0, use_extended_syntax: false } 
    }

    // Query parameter with 'extended syntax,'
    // meaning that = is represented as =eq.,
    // < is represented as =lt. etc.
    pub fn new_query_parameter_ext(name: &str, value: &str, expected_type: ParameterType) -> Self{
        let extension = UnCheckedParam::analyze_extended_val( value );
        if extension.1 == CPRelation::Unknown{ UnCheckedParam::new_err_query_ext_param_with_unknown_relation(name, value)}
        else{
            let check = UnCheckedParam::get_typecheck_of_query_parameter( &extension.0[..], expected_type ); // .1 ist Problem, .0 ist value
            UnCheckedParam { problem: check.1, name: name.to_string(), relation: extension.1, value: check.0, use_extended_syntax: true } 
        }
    }

    // Used for the analysis of query parameters with extended
    // values (constructred through .new_query_parameter_ext)
    //
    // Splits ext_value into an actual value and the relation
    // that the ext_value represents.
    // 
    // ext_value is "eq.7," "lt.0," "ne.Ham" etc.
    // assert_eq!( analyze_extended_val(&"eq.7"), ("7", CPRelation::Bigint))
    // 
    // If extvalue contains no "." character, or if the slice
    // before "." does not represent a known relation, 
    // (ext_value, CPRelation::Unknown) is returned.
    fn analyze_extended_val( ext_value: &str ) -> (String, CPRelation){
        match ext_value.chars().position(|c| c == '.'){
            Some( pos ) => (ext_value.chars().skip(pos+1).collect(),CPRelation::new( &ext_value.chars().take(pos).collect::<String>()[..])),
            _ => (ext_value.to_string(), CPRelation::Unknown )
        }
    }

    // @TODO: required needs to be followed up: the information is 
    // handed over as a parameter here, NEEDS THINKING
    // (Can there be a missing value of a non-required parameter?)
    pub fn new_payload_parameter(name: &str, o_value: Option<&Value>, expected_type: ParameterType, required: bool) -> Self{
         
        match o_value{
            Some( value ) => {
                info!("Creating new payload param with value: >{:?}<", value);
                let check=UnCheckedParam::get_typecheck_of_payload_parameter( value, expected_type );
                UnCheckedParam { problem: check.1, name: name.to_string(), relation: CPRelation::Unknown, value: check.0, use_extended_syntax: false } 
            },
            None => {
                if required{ UnCheckedParam::new_err_missing_parameter(name)}
                else{panic!("Something wrong: there's an unrequired parameter without a value ... ?");}
            }

        }
    }

    // Used if there is a POST/PATCH request but no
    // parameters configured in the API -> there 
    // is no route.
    pub fn new_err_no_route() -> Self{
        UnCheckedParam { problem: "No such route".to_string(), name: S_EMPTY, relation: CPRelation::Unknown, 
            value: ParamVal::Text(S_EMPTY), use_extended_syntax: false } 
    }

    pub fn new_err_missing_parameter( s_name: &str ) -> Self{
        UnCheckedParam { problem: format!("parameter \"{}\" is obligatory according to api, but missing from the request", s_name), 
            name: S_EMPTY, relation: CPRelation::Unknown, value: ParamVal::Text(S_EMPTY), use_extended_syntax: false } 
    }

    pub fn new_err_query_ext_param_with_unknown_relation( s_name: &str, s_value: &str ) -> Self{
        UnCheckedParam { problem: format!("parameter \"{}\" is handed over as \"extended,\" but value \"{}\" does not contain a \
            recognizable relation. (Extended parameter have values such as eq.7 for \"equals 7\")", s_name, s_value), 
            name: S_EMPTY, relation: CPRelation::Unknown, value: ParamVal::Text(S_EMPTY), use_extended_syntax: true } 
    }

    // Parameter is in API, but not marked as required 
    // and not in the request. In short, not a problem.
    pub fn new_err_non_required_parameter_missing() -> Self{
        UnCheckedParam { problem: API::SUPERFLUOUS_PARAMETER.to_string(), name: S_EMPTY, relation: CPRelation::Unknown, 
            value: ParamVal::Text(S_EMPTY), use_extended_syntax: false } 
    }

    pub fn is_conform( &self ) -> bool{
        self.problem.len()==0
    }

    fn get_typecheck_of_payload_parameter( value: &Value, expected_type: ParameterType ) -> (ParamVal, String){
        match expected_type {
            ParameterType::STRING => match value.as_str(){
                // String OR Array OR Object are all converted to ParamVal(Text)
                // in order to transfert them to Postgres
                // @TODO: Arrays and Objects could probably be checked for 
                //        conformity to API and handed to the DB as is
                //        would be a nice asset!
                Some( val ) => (ParamVal::Text(val.to_string()), S_EMPTY),
                None => match value.is_array() || value.is_object(){
                    true => (ParamVal::Text(value.to_string()), S_EMPTY),
                    _ => (ParamVal::Text(S_EMPTY), S_EMPTY),
                }
            },
            ParameterType::INTEGER => match value.is_i64(){
                true => (ParamVal::Int( value.as_i64().unwrap().try_into().unwrap()), S_EMPTY ), // try_into for i64 -> i32. There is no i32 in serde::value
                false => ( ParamVal::Text(S_EMPTY), format!("Not an integer value: `{}`", value))
            }
            ParameterType::BIGINT => match value.is_i64(){
                true => (ParamVal::BigInt( value.as_i64().unwrap()), S_EMPTY),
                false => (ParamVal::Text(S_EMPTY),format!("Not a bigint value: `{}`", value))
            }
            ParameterType::BOOLEAN => match value.is_boolean(){
                true => (ParamVal::Boolean( value.as_bool().unwrap()), S_EMPTY ),
                false => (ParamVal::Text(S_EMPTY),format!("Not a boolean value: `{}`", value))
            }
            ParameterType::NUMBER => match value.is_f64(){
                true => (ParamVal::Float( value.as_f64().unwrap()), S_EMPTY),
                false => (ParamVal::Text(S_EMPTY),format!("Not a float number: `{}`", value))
            }
            _ => (ParamVal::Text(S_EMPTY),format!("Unknown type expected, giving up."))

        }
    }

    fn get_typecheck_of_query_parameter( value: &str, expected_type: ParameterType ) -> (ParamVal, String){
        match expected_type {
            ParameterType::STRING => (ParamVal::Text(value.to_string()), S_EMPTY),
            ParameterType::INTEGER => match value.parse::<i32>().is_ok(){
                true => (ParamVal::Int( value.parse::<i32>().unwrap()), S_EMPTY ),
                false => ( ParamVal::Text(S_EMPTY), format!("Not an integer value: `{}`", value))
            }
            ParameterType::BIGINT => match value.parse::<i64>().is_ok(){
                true => (ParamVal::BigInt( value.parse::<i64>().unwrap()), S_EMPTY),
                false => (ParamVal::Text(S_EMPTY),format!("Not a bigint value: `{}`", value))
            }
            ParameterType::BOOLEAN => match value.parse::<bool>().is_ok(){
                true => (ParamVal::Boolean( value.parse::<bool>().unwrap()), S_EMPTY ),
                false => (ParamVal::Text(S_EMPTY),format!("Not a boolean value: `{}`", value))
            }
            ParameterType::NUMBER => match value.parse::<f64>().is_ok(){
                true => (ParamVal::Float( value.parse::<f64>().unwrap()), S_EMPTY),
                false => (ParamVal::Text(S_EMPTY),format!("Not a float number: `{}`", value))
            }
            _ => (ParamVal::Text(S_EMPTY),format!("Unknown type expected, giving up."))

        }
    }
}

// ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
// MuscleConfiguration parameters
#[derive (Debug)]
pub struct MuscleConfig{
    port: usize,                     // Server port
    addr: String,                    // Server address
    db: String,                      // Name of Postgres (Pg) db
    db_user: String,                 // Name of Pg user
    db_pass: String,                 // Password of Pg user
    cert_pass: String,               // Pwd for server certificate (TLS/Https)
    cert_file: String,               // Certificate file (TLS/Https)
    api_conf: String,                // OpenAPI config file containing endpoints
    static_files_folder: String, // Path to serve static files from
    token_name: String,              // Pg token name: @TODO
    token_secret: String,            // Pg shared token secret: @TODO
    pg_setvar_prefix: String,        // Pg prefix for variables that are set in postgres through the token: @TODO
    timezone: String,                // Timezone to set Pg to
    server_read_timeout_ms: u64,     // Tweak @TODO
    server_read_chunksize: usize,     // Tweak @TODO
    server_use_https: bool,           // Listen for https requests (true) or http?
    client_ip_allow: Ipv4Addr,        //
    use_eq_syntax_on_url_parameters: bool // translate https://url?param=eq.5 to "param=5" (...lt.5 to "param < 5"). @TODO. true not yet implemented (August 24, 21)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>>{
    env_logger::init();
    let args: Vec<String> = env::args().collect();

    if args.len() <= 1{
        error!("Missing command line argument pointing to .ini file");
        panic!("Missing command line argument pointing to .ini file");
    }

    let pg_api_muscle_config = Arc::new(get_conf( &args[1] ));

    // -------------------------------------------------------
    // Set up socket
    let server_base_url = format!("{}:{}", pg_api_muscle_config.addr, pg_api_muscle_config.port);
    let tcp_listener: TcpListener = TcpListener::bind(&server_base_url).await?;

    // Create the TLS acceptor.
    let mut certificat_file = match File::open( &*pg_api_muscle_config.cert_file ){
        Ok ( f ) => f,
        Err ( e ) => panic!("Certificate `{}` not found: {:?}", &*pg_api_muscle_config.cert_file, e)
    };

    let mut identity = vec![];
    certificat_file.read_to_end( &mut identity ).expect(&*format!("Reading certificate file `{}`", pg_api_muscle_config.cert_file));

    let certificate = Identity::from_pkcs12( &identity, &*pg_api_muscle_config.cert_pass ).expect(&*format!("Constructing certificate from file `{}` using password `{}`", pg_api_muscle_config.cert_file, pg_api_muscle_config.cert_pass)); 
    let tls_acceptor = tokio_native_tls::TlsAcceptor::from(native_tls::TlsAcceptor::builder(certificate).build()?);

    // -------------------------------------------------------
    // Set up DEADPOOL
    // See <https://docs.rs/deadpool-postgres/0.7.0/deadpool_postgres/config/struct.Config.html>
    let mut deadpool_config = Config::new();
    deadpool_config.dbname = Some(pg_api_muscle_config.db.to_string());
    deadpool_config.user = Some(pg_api_muscle_config.db_user.to_string());
    deadpool_config.password = Some(pg_api_muscle_config.db_pass.to_string());
    deadpool_config.manager = Some(ManagerConfig { recycling_method: RecyclingMethod::Fast });

//    This (below) does not make sure that the timezone is set on all clients;
//    it may set the timezone on *recycled* clients, but when a new client is
//    initiated into the pool, GMT is set again. Waiting for a future version 
//    of tokio for this, cf.
//    <https://github.com/sfackler/rust-postgres/issues/147#event-4149833164>
//    cfg.manager = Some(ManagerConfig { recycling_method: RecyclingMethod::Custom(format!("set timezone='Europe/Berlin'")) });
//    Nor does this work:
//    cfg.options = Some(format!("-c timezone={}", conf.timezone.to_string()));
    let pool = deadpool_config.create_pool(NoTls).unwrap();

    // adjust_timezone( &mut pool.get().await.unwrap(), "Europe/Berlin").await;
    // DEADPOOL END

    // time_out specifies when to stop waiting for more
    // input from the socket
    let read_timeout = Duration::from_millis( pg_api_muscle_config.server_read_timeout_ms );
    let chunksize = pg_api_muscle_config.server_read_chunksize;
    let muscle_config = Arc::clone( &pg_api_muscle_config );
    let b_check_client_ip = !muscle_config.client_ip_allow.eq(&Ipv4Addr::new(0,0,0,0));

    // API contains basically the main logic, esp. also the
    // routing table. Since the API is handed the request,
    // it needs to be mutable. That's why it is put inside
    // an async-aware Mutex.
    let muscle_api = Arc::new(Mutex::new(
        API::new( &muscle_config.addr, 
            &muscle_config.token_name, 
            &muscle_config.pg_setvar_prefix, 
            &muscle_config.api_conf,
            muscle_config.use_eq_syntax_on_url_parameters
        )));


    info!("~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~");
    info!("Starting pg_api_muscle service");
    info!("Listening to port {}", muscle_config.port);
    info!("Https? {}", muscle_config.server_use_https);
    info!("Connected to database: >{}<", muscle_config.db);
    info!("Restricted to clients from: >{}<", muscle_config.client_ip_allow);
    info!("~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~");

    loop {
        // Asynchronously wait for an inbound socket.
        let (socket, remote_addr) = tcp_listener.accept().await?;
        let tls_acceptor = tls_acceptor.clone();
        info!("Accepting connection from {}", remote_addr);

        let api = Arc::clone(&muscle_api);

        // Need the ip address for logging and to make sure
        // that shutdown requests are only executed if they
        // come from 127.0.0.1. 
        let client_ip = remote_addr.ip().to_string();
        
        if b_check_client_ip {
            if !client_ip.parse::<Ipv4Addr>().unwrap().eq(&muscle_config.client_ip_allow){ 
                info!("Request from >{}< ignored due to client_ip_allow restrictions set in initialisation file", client_ip);
                continue; 
            }
        }

        // Clone things for the spawned thread:
        let cloned_conf = Arc::clone( &pg_api_muscle_config );
        let cloned_pool = pool.clone();

        // Deal with the connection
        tokio::spawn(async move {

            // If the API is configured to listen for https: accept the TLS connection.
            // otherwise get the TcpStream
            let mut var_stream: VarStream = match cloned_conf.server_use_https{
                true =>  {match tls_acceptor.accept(socket).await{
                            Ok( stream ) => VarStream::Secure(stream),
                            Err (e) => {
                                error!("TLS Accept error: {}", e);
                                return;
                            }
                    }
                },
                _ => VarStream::Insecure(socket)
            };

            let mut s_request = String::from(""); // <- String to hold the request

            // Oddly, tls_stream.read( &buffer ) always reads CHUNKS of 16384 bytes (16 kB) and 
            // then stops. Longer payloads are not read. This is why .read() needs to be
            // called in a loop.
            //
            // Unfortunatley, the stream does not recognize EOF. So if a chunk is exactly
            // 16384 bytes long, the loop loops, and the thread *hangs* waiting for
            // more input ... for ever.
            //
            // This is why the .read() is called in a timeout of configurable ms-length
            //
            // Today (April in 2021), I am not sure, if the 16 kB are specific to this 
            // machine? This is why it's also configurable.
            let mut n = chunksize;
            while n == chunksize{
                let mut buffer = vec![0; chunksize];
                n = match tokio::time::timeout(read_timeout, var_stream
                        .read(&mut buffer)).await{
                            Ok( o ) => {
                                let o = o.unwrap();
                                s_request.push_str( &String::from_utf8_lossy( &buffer )); o
                            },
                            Err( e) => {
                                // disambiguate timeout from real error
                                match e.source(){
                                    Some (_sou) => error!("Error reading tcp stream: {:?}", e),
                                    None => error!("Timeout reading request after {} bytes", n)
                                }
                                0
                            }
                        }
            }

            if n == 0 { return; }
            
            // response is 
            //   .0: status + header,
            //   .1: content,
            //   .2: flag for request for static content,
            let mut response = handle_connection(client_ip, 
                s_request, &cloned_pool, 
                &mut api.lock().await.borrow_mut(), &cloned_conf.token_secret,
                &cloned_conf.static_files_folder).await;

            let s_status_and_header = response.0; 
            let v_response = &mut s_status_and_header.into_bytes();

            // static responses get an extra linebreak between headers
            // and the content. Esp. binary format -- .pngs, for example --
            // cause weird browser problems otherwise: Firefox displays the 
            // png but fails on download; chrome does not display.
            // Even with the linebreak, wget is unhappy and complains about
            // a reading error (Lesefehler)
            // anyway -- here it goes. response.2 is boolean for a request to 
            // a static resource
            if response.2 {v_response.push(b'\n');}

            v_response.append( &mut response.1 );

            var_stream
                .write_all( v_response )
                .await
                .expect("failed to write data to socket");

            // @todo: A graceful shutdown would be nicer, but seems connected with 
            // channels or tokio::signal technology, i.e. more complex
            if (api.lock().await.request).is_shutdown{ 
                info!("Shutting down on request.");
                exit(0);
            }
        });
    } // LOOP
}

///
/// Parses the incoming request, 
/// compares its validity against the API,
/// rejects the request if it does not conform to the API,
/// or gets a response from tokio_postgrest as the API specifies.
///
async fn handle_connection(s_client_ip: String, 
    s_request: String, 
    db_client: &Pool, 
    mut api: &mut API, 
    token_secret: &String,
    static_files: &String
) -> (String, Vec<u8>, bool){
    let request = &mut Request::new( &s_request, 
        &s_client_ip,
        &api.local_ip_address, 
        &token_secret,
        &static_files
     );
    api.set_request( &request );
    Response::new( &mut api, db_client ).await.get_response()
}

// =====================================================================================
// Boring code
// =====================================================================================

/**
 * Parse HTTP data into Request Object for better handling
 **/
#[cfg(test)]
mod test_get_query{
    use super::*;

    #[test]
    fn simple() {
        let mut r:Request = Request::new( "/path/to/this?a=1&b=ä", "::1", "127.0.0.1", "", "static" );
        assert_eq!( r.get_query_parameter_value( "a" ),  Some("1") );
        assert_eq!( r.get_query_parameter_value( "b" ),  Some("ä") );
        assert_eq!( r.get_query_parameter_value( "c" ),  None );
        assert_eq!( r.get_payload_param( "c" ),  None );
        assert_eq!( r.is_static(),  false );
    }
    #[test]
    fn simple1() {
        let mut r:Request = Request::new( "path/to/this", "::1", "127.0.0.1", "", "static");
        assert_eq!( r.get_query_parameter_value( "a"),  None );
        assert_eq!( r.get_query_parameter_value( "c" ),  None );
        assert_eq!( r.get_payload_param( "c" ),  None );
        assert_eq!( r.is_static(),  false );
    }

    #[test]
    fn s_payload() {
        let mut r:Request = Request::new( "path/to/this\n{\"this\":\"that\"}", "::1", "127.0.0.1", "" , "static");
        assert_eq!( r.get_query_parameter_value( "a"),  None );
        assert_eq!( r.get_query_parameter_value( "c" ),  None );
        assert_eq!( r.get_payload_param( "this" ).unwrap().as_str(),  Some("that") );
        assert_eq!( r.is_static(),  false );

    }

    #[test]
    fn get_static() {
        let r:Request = Request::new( "/static/path/to/this\n{\"this\":\"that\"}", "::1", "127.0.0.1", "" , "static");
        assert_eq!( r.is_static(),  true );
    }

    #[test]
    fn get_auth() {
        let r:Request = Request::new( "/static/path/to/this\n{\"this\":\"that\"}\nAuthorization: Bearer 1234&äß", "::1", "127.0.0.1", "" , "static");
        assert_eq!( r.get_auth(),  "1234&äß" );
//        assert_eq!( r.has_token(),  true );
    }

    #[test]
    fn get_auth_problematic_short() {
        let r:Request = Request::new( "/static/path/to/this\n{\"this\":\"that\"}\nAuthorization: Bearer", "::1", "127.0.0.1", "", "static");
        assert_eq!( r.get_auth(),  "" );
//        assert_eq!( r.has_token(),  false );
    }

    #[test]
    fn get_auth_problematic_long() {
        let r:Request = Request::new( "/static/path/to/this\n{\"this\":\"that\"}\nAuthorization: Bearer 1234567890123456789012345678901234567890", "::1", "127.0.0.1", "" , "static");
        assert_eq!( r.get_auth(),  "1234567890123456789012345678901234567890" );
//        assert_eq!( r.has_token(),  true );
    }

    #[test]
    fn s_broken_payload() {
        let mut r:Request = Request::new( "path/to/this\n{\"this\":\"that\"", "::1", "127.0.0.1", "" , "static");
        assert_eq!( r.get_query_parameter_value( "a"),  None );
        assert_eq!( r.get_query_parameter_value( "c" ),  None );
        assert_eq!( r.get_payload_param( "this" ),  None );
        assert_eq!( r.is_static(),  false );

    }
    }

fn get_conf( s_file: &str ) -> MuscleConfig{

    let conf = match Ini::from_file( s_file ){
        Ok( a ) => a,
        Err ( e ) => panic!("Configuration file `{}` not found or not accessible: {:?}", s_file, e)
    };

    let s_err = format!("Configuration file `{}` is missing this entry: ", s_file);

    MuscleConfig{
        db: conf.get("Database", "db").expect(
            &format!("{}{}", s_err, "`db` in section `Database`")[..]),

        db_user: conf.get("Database", "db_user").expect(
            &format!("{}{}", s_err, "`db_user` in section `Database`")[..]),
            
        db_pass: conf.get("Database", "db_pass").expect(
            &format!("{}{}", s_err, "`db_pass` in section `Database`")[..]),
            
        timezone: conf.get("Database", "timezone").expect(
            &format!("{}{}", s_err, "`timezone` in section `Database`")[..]),

        port: conf.get("Webservice", "port").expect(
            &format!("{}{}", s_err, "`port` in section `Webservice`")[..]),

        addr: conf.get("Webservice", "addr").expect(
            &format!("{}{}", s_err, "`addr` in section `Webservice`")[..]),

        server_read_timeout_ms: conf.get("Webservice", "server_read_timeout_ms").expect(
            &format!("{}{}", s_err, "`server_read_timeout_ms` in section `Webservice`")[..]),
            
        server_read_chunksize: conf.get("Webservice", "server_read_chunksize").expect(
            &format!("{}{}", s_err, "`server_read_chunksize` in section `Webservice`")[..]),

        server_use_https: conf.get("Webservice", "https").expect(
            &format!("{}{}", s_err, "`https` in section `Webservice`")[..]),

        client_ip_allow: conf.get("Webservice", "client_ip_allow").expect(
            &format!("{}{}", s_err, "`https` in section `Webservice`")[..]),

        cert_pass: conf.get("Webservice", "cert_pass").expect(
            &format!("{}{}", s_err, "`cert_pass` in section `Webservice`")[..]),

        cert_file: conf.get("Webservice", "cert_file").expect(
            &format!("{}{}", s_err, "`cert_file` in section `Webservice`")[..]),

        api_conf: conf.get("Webservice", "api_conf").expect(
            &format!("{}{}", s_err, "`api_conf` in section `Webservice`")[..]),

        static_files_folder: conf.get("Webservice", "static_files_folder").expect(
            &format!("{}{}", s_err, "`static_files_folder` in section `Webservice`")[..]),

        token_name: conf.get("Authorization", "pg_token_name").expect(
            &format!("{}{}", s_err, "`pg_token_name` in section `Authorization`")[..]),

        token_secret: conf.get("Authorization", "pg_token_secret").expect(
            &format!("{}{}", s_err, "`pg_token_secret` in section `Authorization`")[..]),

        pg_setvar_prefix: conf.get("Authorization", "pg_setvar_prefix").expect(
            &format!("{}{}", s_err, "`pg_setvar_prefix` in section `Authorization`")[..]),

        use_eq_syntax_on_url_parameters: conf.get("Service", "api_use_eq_syntax_on_url_parameters").expect(
            &format!("{}{}", s_err, "`api_use_eq_syntax_on_url_parameters` in section `Service`")[..])
    }
}

#[cfg(test)]
mod test_query_parameters{
    use super::*;

    #[test]
    fn simple() {
        //let t=UnCheckedParam::new_query_parameter("test", String("b"), "", true);
        let t=UnCheckedParam::new_query_parameter("test", "5", ParameterType::BIGINT);
        assert_eq!(t.is_conform(), true);

        let t=UnCheckedParam::new_query_parameter("test", "eq.5", ParameterType::BIGINT);
        assert_eq!(t.is_conform(), false);

        let t=UnCheckedParam::new_query_parameter_ext("test", "eq.8", ParameterType::BIGINT);
        assert_eq!(t.is_conform(), true);

        let t=UnCheckedParam::new_query_parameter_ext("test", "lt.8", ParameterType::BIGINT);
        assert_eq!(t.is_conform(), true);

        let t=UnCheckedParam::new_query_parameter_ext("test", "le.8", ParameterType::BIGINT);
        assert_eq!(t.is_conform(), true);

        let t=UnCheckedParam::new_query_parameter_ext("test", "gt.8", ParameterType::BIGINT);
        assert_eq!(t.is_conform(), true);

        let t=UnCheckedParam::new_query_parameter_ext("test", "ur.8", ParameterType::BIGINT);
        assert_eq!(t.is_conform(), false);

        let t=UnCheckedParam::new_query_parameter_ext("test", "eq.true", ParameterType::BOOLEAN);
        assert_eq!(t.is_conform(), true);

        let t=UnCheckedParam::new_query_parameter_ext("test", "eq.true2", ParameterType::BOOLEAN);
        assert_eq!(t.is_conform(), false);

        let t=UnCheckedParam::new_query_parameter_ext("test", "eq.Horst", ParameterType::STRING);
        assert_eq!(t.is_conform(), true);

        let t=UnCheckedParam::new_query_parameter_ext("test", "ne.Horst", ParameterType::STRING);
        assert_eq!(t.is_conform(), true);

        let t=UnCheckedParam::new_query_parameter_ext("test", "ne.7.889", ParameterType::NUMBER);
        assert_eq!(t.is_conform(), true);

        let t=UnCheckedParam::new_query_parameter_ext("test", "ne.7.8.89", ParameterType::NUMBER);
        assert_eq!(t.is_conform(), false);

        let t=UnCheckedParam::new_query_parameter_ext("test", "ne.7.889", ParameterType::BIGINT);
        assert_eq!(t.is_conform(), false);

        let t=UnCheckedParam::new_query_parameter_ext("test", "eqtrue2", ParameterType::BOOLEAN);
        assert_eq!(t.is_conform(), false);

        let t=UnCheckedParam::new_query_parameter_ext("test", "eq.a8", ParameterType::BIGINT);
        assert_eq!(t.is_conform(), false);
    }
}
