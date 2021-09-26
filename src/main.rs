use std::{borrow::BorrowMut, env, error::Error, fmt::{self}, fs::File, io::prelude::*, net::{Ipv4Addr}, process::exit, sync::Arc};
use futures::lock::Mutex;
use native_tls::Identity;
use tokio::{io::{AsyncReadExt, AsyncWriteExt}, net::TcpStream};
use tokio::net::TcpListener;
use tokio_native_tls::TlsStream;
use self::request::Request;
use self::response::Response;
use self::{config::MuscleConfig, api::API, parameter::{CheckedParam, ParameterToCheck, ParameterType}};
use log::{error, info, debug};
use std::time::Duration;
use deadpool_postgres::{Config, ManagerConfig, Pool, RecyclingMethod };
use tokio_postgres::NoTls;

mod db;
mod parameter;
mod request;
mod response;
mod api;
mod config;

#[macro_use]
extern crate serde;
extern crate log;
extern crate serde_json;

pub const S_EMPTY: String = String::new();

// Helper so that http and https connections can
// be handled under one umbrella stream: "VarStream"
// Exposes read and write_all
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


// ----------------------------------------------------------------------------------------
// Structs


/// If .ini has `api_use_eq_syntax_on_url_parameters=true`,
/// (enabling http.../url?param=eq.1&...)
/// this enum lists the possible relations, eq, lt etc.
/// @todo: LIKE and IN are not configured through to db
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


#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>>{
    env_logger::init();
    let args: Vec<String> = env::args().collect();

    if args.len() <= 1{
        error!("Missing command line argument. You need to specify a configuration file.");
        panic!("Missing command line argument. You need to specify a configuration file.");
    }

    // Panics if file not found or erroneous config
    let pg_api_muscle_config = Arc::new(MuscleConfig::new( &args[1] ));

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
    // -------------------------------------------------------

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


        // Need the ip address for logging and to make sure
        // that shutdown and reload requests are only executed if they
        // come from 127.0.0.1. 
        let client_ip = remote_addr.ip().to_string();
        
        // Configuration may restrict requests to a single IP 
        // address (for reverse proxy use)
        // @TODO allow wildcards, lists etc.
        if b_check_client_ip {
            if !client_ip.parse::<Ipv4Addr>().unwrap().eq(&muscle_config.client_ip_allow){ 
                debug!("Request from >{}< ignored due to client_ip_allow restrictions set in initialisation file", client_ip);
                continue; 
            }
        }

        // Clone some objects things for the spawned thread:
        let spawned_api = Arc::clone(&muscle_api);
        let spawned_conf = Arc::clone( &pg_api_muscle_config );
        let spawned_pool = pool.clone();

        // Deal with the connection
        tokio::spawn(async move {

            // If the API is configured to listen for https: accept the TLS connection.
            // otherwise get the TcpStream
//            let mut var_stream: VarStream = match cloned_conf.server_use_https{
            let mut var_stream: VarStream = match spawned_conf.server_use_https{
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
            
            let mut response = handle_connection(client_ip, 
                s_request, &spawned_pool, 
                &mut spawned_api.lock().await.borrow_mut(), &spawned_conf).await;

            let v_response = &mut response.http_status_len_header.into_bytes();

            // static responses get an extra linebreak between headers
            // and the content. Esp. binary format -- .pngs, for example --
            // cause weird browser problems otherwise: Firefox displays the 
            // png but fails on download; chrome does not display.
            // Even with the linebreak, wget is unhappy and complains about
            // a reading error (Lesefehler)
            // anyway -- here it goes. response.2 is boolean for a request to 
            // a static resource
            if response.is_static { v_response.push(b'\n'); }

            v_response.append( &mut response.http_content );

            var_stream
                .write_all( v_response )
                .await
                .expect("failed to write data to socket");

            // @todo: A graceful shutdown would be nicer, but seems connected with 
            // channels or tokio::signal technology, i.e. more complex
            if (spawned_api.lock().await.request).is_shutdown{ 
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
    conf: &MuscleConfig
) -> Response{
    let request = &mut Request::new( &s_request, 
        &s_client_ip,
        &api.local_ip_address, 
        &conf.token_secret,
        &conf.static_files_folder
     );
    api.set_request( &request );
    Response::new( &mut api, db_client, &conf ).await
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


#[cfg(test)]
mod test_query_parameters{
    use super::*;

    #[test]
    fn simple() {
        //let t=UnCheckedParam::new_query_parameter("test", String("b"), "", true);
        let t=ParameterToCheck::new_query_parameter("test", "5", ParameterType::BIGINT);
        assert_eq!(t.is_conform(), true);

        let t=ParameterToCheck::new_query_parameter("test", "eq.5", ParameterType::BIGINT);
        assert_eq!(t.is_conform(), false);

        let t=ParameterToCheck::new_query_parameter_ext("test", "eq.8", ParameterType::BIGINT);
        assert_eq!(t.is_conform(), true);

        let t=ParameterToCheck::new_query_parameter_ext("test", "lt.8", ParameterType::BIGINT);
        assert_eq!(t.is_conform(), true);

        let t=ParameterToCheck::new_query_parameter_ext("test", "le.8", ParameterType::BIGINT);
        assert_eq!(t.is_conform(), true);

        let t=ParameterToCheck::new_query_parameter_ext("test", "gt.8", ParameterType::BIGINT);
        assert_eq!(t.is_conform(), true);

        let t=ParameterToCheck::new_query_parameter_ext("test", "ur.8", ParameterType::BIGINT);
        assert_eq!(t.is_conform(), false);

        let t=ParameterToCheck::new_query_parameter_ext("test", "eq.true", ParameterType::BOOLEAN);
        assert_eq!(t.is_conform(), true);

        let t=ParameterToCheck::new_query_parameter_ext("test", "eq.true2", ParameterType::BOOLEAN);
        assert_eq!(t.is_conform(), false);

        let t=ParameterToCheck::new_query_parameter_ext("test", "eq.Horst", ParameterType::STRING);
        assert_eq!(t.is_conform(), true);

        let t=ParameterToCheck::new_query_parameter_ext("test", "ne.Horst", ParameterType::STRING);
        assert_eq!(t.is_conform(), true);

        let t=ParameterToCheck::new_query_parameter_ext("test", "ne.7.889", ParameterType::NUMBER);
        assert_eq!(t.is_conform(), true);

        let t=ParameterToCheck::new_query_parameter_ext("test", "ne.7.8.89", ParameterType::NUMBER);
        assert_eq!(t.is_conform(), false);

        let t=ParameterToCheck::new_query_parameter_ext("test", "ne.7.889", ParameterType::BIGINT);
        assert_eq!(t.is_conform(), false);

        let t=ParameterToCheck::new_query_parameter_ext("test", "eqtrue2", ParameterType::BOOLEAN);
        assert_eq!(t.is_conform(), false);

        let t=ParameterToCheck::new_query_parameter_ext("test", "eq.a8", ParameterType::BIGINT);
        assert_eq!(t.is_conform(), false);
    }
}
