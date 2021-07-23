use std::{
    fmt::{self, Formatter, Display},
    sync::Arc,
    path::Path,
    io::BufReader,
    env,
    fs::File,
    error::Error,
    io::prelude::*
};
// use std::convert::TryInto;
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

//#[json]
use serde_json::Value;
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

// ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
// UnCheckedParam are compared against
// API and transformed to CheckedParam if
// they contain no problems.
#[derive(Debug, Clone)]
pub struct UnCheckedParam{
    problem: String,
    name: String,
    value: ParamVal
}

#[derive(Debug, Clone)]
pub struct CheckedParam{
    name: String,
    value: ParamVal
}

// ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
// MuscleConfiguration parameters
#[derive (Debug)]
pub struct MuscleConfig{
    port: usize,
    addr: String,
    db: String,
    db_user: String,
    db_pass: String,
    cert_pass: String,
    cert_file: String,
    api_conf: String,
    token_name: String,
    token_secret: String,
    pg_setvar_prefix: String,
    timezone: String,
    server_read_timeout_ms: u64,
    server_read_chunksize: usize
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>>{
    env_logger::init();
    let args: Vec<String> = env::args().collect();

    if args.len() <= 1{
        error!("Missing command line argument pointing to .ini file");
        panic!("Missing command line argument pointing to .ini file");
    }

    let conf_arc = Arc::new(get_conf( &args[1] ));

    // -------------------------------------------------------
    // Set up socket
    let addr = format!("{}:{}", conf_arc.addr, conf_arc.port);
    let tcp: TcpListener = TcpListener::bind(&addr).await?;

    // Create the TLS acceptor.
    let mut file = match File::open( &*conf_arc.cert_file ){
        Ok ( f ) => f,
        Err ( e ) => panic!("Certificate `{}` not found: {:?}", &*conf_arc.cert_file, e)
    };

    let mut identity = vec![];
    file.read_to_end( &mut identity ).expect(&*format!("Reading certificate file `{}`", conf_arc.cert_file));

    let cert = Identity::from_pkcs12( &identity, &*conf_arc.cert_pass ).expect(&*format!("Constructing certificate from file `{}` using password `{}`", conf_arc.cert_file, conf_arc.cert_pass)); 

    let tls_acceptor = tokio_native_tls::TlsAcceptor::from(native_tls::TlsAcceptor::builder(cert).build()?);

    // -------------------------------------------------------
    // Set up DEADPOOL
    // See <https://docs.rs/deadpool-postgres/0.7.0/deadpool_postgres/config/struct.Config.html>
    let mut cfg = Config::new();
    cfg.dbname = Some(conf_arc.db.to_string());
    cfg.user = Some(conf_arc.db_user.to_string());
    cfg.password = Some(conf_arc.db_pass.to_string());
    cfg.manager = Some(ManagerConfig { recycling_method: RecyclingMethod::Fast });
//    This (below) does not make sure that the timezone is set on all clients;
//    it may set the timezone on *recycled* clients, but when a new client is
//    initiated into the pool, GMT is set again. Waiting for a future version 
//    of tokio for this, cf.
//    <https://github.com/sfackler/rust-postgres/issues/147#event-4149833164>
//    cfg.manager = Some(ManagerConfig { recycling_method: RecyclingMethod::Custom(format!("set timezone='Europe/Berlin'")) });
//    Nor does this work:
//    cfg.options = Some(format!("-c timezone={}", conf.timezone.to_string()));
    let pool = cfg.create_pool(NoTls).unwrap();

    // adjust_timezone( &mut pool.get().await.unwrap(), "Europe/Berlin").await;
    // DEADPOOL END

    // time_out specifies when to stop waiting for more
    // input from the socket
    let read_timeout = Duration::from_millis( conf_arc.server_read_timeout_ms );
    let chunksize = conf_arc.server_read_chunksize;
    let api_val_rc = Arc::new( read_api(&conf_arc.api_conf));

    loop {
        // Asynchronously wait for an inbound socket.
        let (socket, remote_addr) = tcp.accept().await?;
        let tls_acceptor = tls_acceptor.clone();
        info!("Accepting connection from {}", remote_addr);

        // Clone things for the spawned thread:
        let conf = Arc::clone( &conf_arc );
        let api_val = Arc::clone( &api_val_rc );
        let pool = pool.clone();
        let mut api = API::new( &conf.token_name, &conf.pg_setvar_prefix ); 

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
            
            let mut response = handle_connection(s_request, &pool, &mut api, &api_val, &conf.token_secret).await;
            let r1 = response.0; 
            let tt = &mut r1.into_bytes();
            &tt.push(b'\n'); // extra line-break is important before binary input.
            &tt.append( &mut response.1 );

            tls_stream
                .write_all( &tt )
                .await
                .expect("failed to write data to socket");
            // tls_stream.flush() ... ?
        });
    }
}
///
/// Parses the incoming request, 
/// compares its validity against the API,
/// rejects the request if it does not conform to the API,
/// or gets a response from tokio_postgrest as the API specifies.
///
async fn handle_connection(s_req: String, cl: &Pool, mut api_shj: &mut API, api: &Value, token_secret: &String) -> (String, Vec<u8>){
    //  @TODO: get ip address
    let s_ip_addr_client = "127.0.0.1";
    let request = &mut Request::new( &s_req, &s_ip_addr_client, &token_secret );
    api_shj.set_request( &request, api );

    Response::new( &mut api_shj, cl, api ).await.get_response()
}

fn read_api<P: AsRef<Path>>(path: P) -> Value {

    // Open the file in read-only mode 
    let file = match File::open(path){
        Err( _e ) => panic!("Cannot find file with API configuration"),
        Ok ( f ) => f
    };

    match serde_json::from_reader( BufReader::new( file )){
        Err( _e ) => panic!("Cannot parse file with API configuration"),
        Ok( api ) => api
    }
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
        let mut r:Request = Request::new( "/path/to/this?a=1&b=ä", "::1", "" );
        assert_eq!( r.get_query_param( "a" ),  Some("1") );
        assert_eq!( r.get_query_param( "b" ),  Some("ä") );
        assert_eq!( r.get_query_param( "c" ),  None );
        assert_eq!( r.get_payload_param( "c" ),  None );
        assert_eq!( r.is_static(),  false );
    }
    #[test]
    fn simple1() {
        let mut r:Request = Request::new( "path/to/this", "::1", "" );
        assert_eq!( r.get_query_param( "a"),  None );
        assert_eq!( r.get_query_param( "c" ),  None );
        assert_eq!( r.get_payload_param( "c" ),  None );
        assert_eq!( r.is_static(),  false );
    }

    #[test]
    fn s_payload() {
        let mut r:Request = Request::new( "path/to/this\n{\"this\":\"that\"}", "::1", "" );
        assert_eq!( r.get_query_param( "a"),  None );
        assert_eq!( r.get_query_param( "c" ),  None );
        assert_eq!( r.get_payload_param( "this" ).unwrap().as_str(),  Some("that") );
        assert_eq!( r.is_static(),  false );

    }

    #[test]
    fn get_static() {
        let r:Request = Request::new( "/static/path/to/this\n{\"this\":\"that\"}", "::1", "" );
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
        let r:Request = Request::new( "/static/path/to/this\n{\"this\":\"that\"}\nAuthorization: Bearer 1234&äß", "::1", "" );
        assert_eq!( r.get_auth(),  "1234&äß" );
//        assert_eq!( r.has_token(),  true );
    }

    #[test]
    fn get_auth_problematic_short() {
        let r:Request = Request::new( "/static/path/to/this\n{\"this\":\"that\"}\nAuthorization: Bearer", "::1", "");
        assert_eq!( r.get_auth(),  "" );
//        assert_eq!( r.has_token(),  false );
    }

    #[test]
    fn get_auth_problematic_long() {
        let r:Request = Request::new( "/static/path/to/this\n{\"this\":\"that\"}\nAuthorization: Bearer 1234567890123456789012345678901234567890", "::1", "" );
        assert_eq!( r.get_auth(),  "1234567890123456789012345678901234567890" );
//        assert_eq!( r.has_token(),  true );
    }

    #[test]
    fn s_broken_payload() {
        let mut r:Request = Request::new( "path/to/this\n{\"this\":\"that\"", "::1", "" );
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
            &format!("{}{}", s_err, "`pg_setvar_prefix` in section `Authorization`")[..])
    }
}
