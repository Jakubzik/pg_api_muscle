use std::{borrow::BorrowMut, convert::TryInto, env, error::Error, fmt::{self, Formatter, Display}, fs::File, io::prelude::*, process::exit, sync::Arc};
use futures::lock::Mutex;
use tini::Ini;
use native_tls::Identity;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
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

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RequestMethod{
    GET,
    POST,
    PATCH,
    DELETE,
    POSTasGET,
    SHUTDOWN,
    RELOAD,
    UNKNOWN
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
            RequestMethod::POSTasGET => write!(f, "Http POST -> GET"),
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

#[derive(Debug, Clone)]
pub enum ParamVal {
    Int(i32),
    BigInt(i64),
    Float(f64),
    Text(String),
    Date(String),
    Boolean(bool),
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

#[derive(Serialize, Clone, Deserialize, Debug)]
pub enum CPRelation{
    Unknown,
    Equal,
    LessThan,
    GreaterThan,
    LessOrEqual,
    GreaterOrEqual,
    Like,
    In
}

impl CPRelation{
    fn db_representation(&mut self) -> &str{
        match self {
            CPRelation::Unknown => "",
            CPRelation::Equal => "=",
            CPRelation::LessThan=> "<",
            CPRelation::GreaterThan => ">",
            CPRelation::LessOrEqual=> "<=",
            CPRelation::GreaterOrEqual=> ">=",
            CPRelation::Like => " LIKE ",
            CPRelation::In => " IN "
        }
    }

//    pub fn new( s_param_name: &str ) -> Self{
//        if s_param_name.len() < 3{ CPRelation::Unknown }
//        let pt = s_param_name.find(".");
//        match s_param_name.chars().take(pt){
//            "eq" => CPRelation::Equal,
//            "lt" => CPRelation::LessThan,
//            "le" => CPRelation::LessOrEqual,
//            "ge" => CPRelation::GreaterOrEqual,
//            "gt" => CPRelation::GreaterThan,
//            "li" => CPRelation::Like,
//            "in" => CPRelation::In,
//            _ => CPRelation::Unknown
//        }
//    }

    fn url_representation(&mut self) -> &str{
        match self {
            CPRelation::Unknown => "",
            CPRelation::Equal => "eq.",
            CPRelation::LessThan=> "lt.",
            CPRelation::GreaterThan => "gt.",
            CPRelation::LessOrEqual=> "le.",
            CPRelation::GreaterOrEqual=> "ge.",
            CPRelation::Like => "li.",
            CPRelation::In => "in."
        }
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
#[derive(Debug, Default, Clone)]
pub struct UnCheckedParam{
    problem: String,
    name: String,
    relation: CPRelation,
    value: ParamVal
}

#[derive(Debug, Clone)]
pub struct CheckedParam{
    name: String,
    relation: CPRelation,
    value: ParamVal
}

impl CheckedParam {
    pub fn new(name: String, value: ParamVal) -> Self { CheckedParam { name, relation: CPRelation::Unknown, value } }
}

impl UnCheckedParam{
    pub fn new(name: String, value: ParamVal, problem: String) -> Self { UnCheckedParam { problem, name, relation: CPRelation::Unknown, value } }
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
    token_name: String,              // Pg token name: @TODO
    token_secret: String,            // Pg shared token secret: @TODO
    pg_setvar_prefix: String,        // Pg prefix for variables that are set in postgres through the token: @TODO
    timezone: String,                // Timezone to set Pg to
    server_read_timeout_ms: u64,     // Tweak @TODO
    server_read_chunksize: usize,     // Tweak @TODO
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

    // API contains basically the main logic, esp. also the
    // routing table. Since the API is handed the request,
    // it needs to be mutable. That's why it is put inside
    // an async-aware Mutex.
    let muscle_api = Arc::new(Mutex::new(
        API::new( &muscle_config.addr, &muscle_config.token_name, &muscle_config.pg_setvar_prefix, &muscle_config.api_conf )));

    loop {
        // Asynchronously wait for an inbound socket.
        let (socket, remote_addr) = tcp_listener.accept().await?;
        let tls_acceptor = tls_acceptor.clone();
        info!("Accepting connection from {}", remote_addr);

        let api = Arc::clone(&muscle_api);

        // Need the ip address for logging and to make sure
        // that shutdown requests are only executed if they
        // come from 127.0.0.1. <- @HACK: server.addr
        let client_ip = remote_addr.ip().to_string();

        // Clone things for the spawned thread:
        let cloned_conf = Arc::clone( &pg_api_muscle_config );
        let cloned_pool = pool.clone();

        // Deal with the connection
        tokio::spawn(async move {

            // Accept the TLS connection.
            let mut tls_stream = match tls_acceptor.accept(socket).await{
                Ok( stream ) => stream,
                Err (e) => {
                    error!("TLS Accept error: {}", e);
                    return;
                }
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
            // Today (April 21), I am not sure, if the 16 kB are specific to this 
            // machine? This is why it's also configurable.
            // @TODO: Catch timeout-error!
            let mut n = chunksize;
            while n == chunksize{
                let mut buffer = vec![0; chunksize];
                n = match tokio::time::timeout(read_timeout, tls_stream
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
            if n == 0 {
                return;
            }
            
            // response is 
            //   .0: status + header,
            //   .1: content,
            //   .2: flag for request for static content,
            let mut response = handle_connection(client_ip, 
                s_request, &cloned_pool, 
                &mut api.lock().await.borrow_mut(), &cloned_conf.token_secret).await;

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
            if response.2 {&v_response.push(b'\n');}

            &v_response.append( &mut response.1 );

            tls_stream
                .write_all( &v_response )
                .await
                .expect("failed to write data to socket");

            // @todo: A graceful shutdown would be nicer, but seems connected with 
            // channels or tokio::signal technology, i.e. more complex
            if (api.lock().await.request).is_shutdown{ 
                info!("Shutting down on request.");
                exit(0);
            }
        });
    }
}

///
/// Parses the incoming request, 
/// compares its validity against the API,
/// rejects the request if it does not conform to the API,
/// or gets a response from tokio_postgrest as the API specifies.
///
async fn handle_connection(s_client_ip: String, s_request: String, db_client: &Pool, mut api: &mut API, token_secret: &String) -> (String, Vec<u8>, bool){
    let request = &mut Request::new( &s_request, &s_client_ip, &api.local_ip_address, &token_secret );
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
        let mut r:Request = Request::new( "/path/to/this?a=1&b=ä", "::1", "127.0.0.1", "" );
        assert_eq!( r.get_query_param( "a" ),  Some("1") );
        assert_eq!( r.get_query_param( "b" ),  Some("ä") );
        assert_eq!( r.get_query_param( "c" ),  None );
        assert_eq!( r.get_payload_param( "c" ),  None );
        assert_eq!( r.is_static(),  false );
    }
    #[test]
    fn simple1() {
        let mut r:Request = Request::new( "path/to/this", "::1", "127.0.0.1", "" );
        assert_eq!( r.get_query_param( "a"),  None );
        assert_eq!( r.get_query_param( "c" ),  None );
        assert_eq!( r.get_payload_param( "c" ),  None );
        assert_eq!( r.is_static(),  false );
    }

    #[test]
    fn s_payload() {
        let mut r:Request = Request::new( "path/to/this\n{\"this\":\"that\"}", "::1", "127.0.0.1", "" );
        assert_eq!( r.get_query_param( "a"),  None );
        assert_eq!( r.get_query_param( "c" ),  None );
        assert_eq!( r.get_payload_param( "this" ).unwrap().as_str(),  Some("that") );
        assert_eq!( r.is_static(),  false );

    }

    #[test]
    fn get_static() {
        let r:Request = Request::new( "/static/path/to/this\n{\"this\":\"that\"}", "::1", "127.0.0.1", "" );
        assert_eq!( r.is_static(),  true );
    }

//    #[test]
//    fn get_url_plus_parms() {
//        let r:Request = Request::new( "Whater?this=that&a=b", "::1" );
//        assert_eq!( Request::get_url_plus_parms("Whater?this=that&a=b").1, "Whatever" );
//
//    }
    #[test]
    fn get_auth() {
        let r:Request = Request::new( "/static/path/to/this\n{\"this\":\"that\"}\nAuthorization: Bearer 1234&äß", "::1", "127.0.0.1", "" );
        assert_eq!( r.get_auth(),  "1234&äß" );
//        assert_eq!( r.has_token(),  true );
    }

    #[test]
    fn get_auth_problematic_short() {
        let r:Request = Request::new( "/static/path/to/this\n{\"this\":\"that\"}\nAuthorization: Bearer", "::1", "127.0.0.1", "");
        assert_eq!( r.get_auth(),  "" );
//        assert_eq!( r.has_token(),  false );
    }

    #[test]
    fn get_auth_problematic_long() {
        let r:Request = Request::new( "/static/path/to/this\n{\"this\":\"that\"}\nAuthorization: Bearer 1234567890123456789012345678901234567890", "::1", "127.0.0.1", "" );
        assert_eq!( r.get_auth(),  "1234567890123456789012345678901234567890" );
//        assert_eq!( r.has_token(),  true );
    }

    #[test]
    fn s_broken_payload() {
        let mut r:Request = Request::new( "path/to/this\n{\"this\":\"that\"", "::1", "127.0.0.1", "" );
        assert_eq!( r.get_query_param( "a"),  None );
        assert_eq!( r.get_query_param( "c" ),  None );
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

        cert_pass: conf.get("Webservice", "cert_pass").expect(
            &format!("{}{}", s_err, "`cert_pass` in section `Webservice`")[..]),

        cert_file: conf.get("Webservice", "cert_file").expect(
            &format!("{}{}", s_err, "`cert_file` in section `Webservice`")[..]),

        api_conf: conf.get("Webservice", "api_conf").expect(
            &format!("{}{}", s_err, "`api_conf` in section `Webservice`")[..]),

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
