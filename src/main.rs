use std::{borrow::{BorrowMut}, collections::HashMap, env, error::Error, fmt::{self}, fs::File, io::prelude::*, net::Ipv4Addr, process::exit, sync::Arc};
//use futures::lock::Mutex;
use tokio::sync::Mutex;
use tokio::sync::RwLock;
//use jwt_simple::prelude::coarsetime::Instant;
use std::time::Instant;
use native_tls::Identity;
use tokio::{io::{AsyncReadExt, AsyncWriteExt}, net::TcpStream};
use tokio::net::TcpListener;
use tokio_native_tls::TlsStream;
use self::request::Request;
use self::response::Response;
use self::{config::MuscleConfigCommon, api::API, parameter::{CheckedParam, ParameterToCheck, ParameterType}};
use log::{debug, error, info};
use std::time::Duration;
use deadpool_postgres::{Config, ManagerConfig, Pool, RecyclingMethod};
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

// Umbrella to handle both http and https connections 
// in one struct.
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

#[derive(Clone)]
pub struct ResponseCacheElement{
    pub cached_response: Response,
    pub timestamp: Instant,
    pub life_span: Duration, 
    pub no_hit: bool
}

impl ResponseCacheElement {
   pub fn new( r: Response ) -> Self{
       Self{
           cached_response: r,
           timestamp: Instant::now(),
           no_hit: false,
           life_span: Duration::from_secs(10) //@TODO make configurable: 10 seconds for now
       }
   }

   pub fn is_expired( &self ) -> bool{
       Instant::now().duration_since( self.timestamp ) > self.life_span
   }

//   //@TODO: probably not needed
//   pub fn no_hit( ) -> Self{
//       Self{
//           cached_response: Response::new_404(),
//           timestamp: Instant::now(),
//           no_hit: true
//       }
//   }
}

#[derive(Clone)]
pub struct ResponseCache{
    pub cache: HashMap<String, ResponseCacheElement>,
    pub size: usize,
    pub size_limit: usize
}

impl ResponseCache{
    pub fn new() -> Self{
        Self{
            cache:HashMap::new(),
            size: 0,
            size_limit: 1 * 1024 * 1024  // 500 MB --> @TODO, make configurable
        }
    }

    // needs to check for duplicates, calculate sizes etc.
    pub fn add( &mut self, url: String, r: Response ){
        if self.cache.get( &url ).is_none(){
            let r_size = &r.http_content.len();
            if self.size + r_size > self.size_limit{
                info!("Cache: limit of {} reached, trying purge before adding new response.", self.size_limit);
                self.purge_expired_responses();
            }
            if self.size + r_size > self.size_limit{
                info!("Cache: limit of {} still exceeded after purge, cannot add `{}`", self.size_limit, &url);
            }else{
                self.size = self.size + r_size;
                debug!("Cache: adding to cache: `{}`, size is now: {} MB", url, self.get_size_mb());
                self.cache.insert(url, ResponseCacheElement::new( r ) );
            }
        }else{
            debug!("Cache: already in cache: `{}`", url);
        }
    }

    // Cleans the cache of all expired responses
    pub fn purge_expired_responses( &mut self ){
        // Changes self.cache and self.size while iterating through self.cache.
        // Have to take cache and size out of self to avoid borrow-trouble:
        let mut local_cache = std::mem::take(&mut self.cache);
        let mut local_size = self.size;

        self.cache.iter().for_each( | resp | {
            if resp.1.is_expired() { 
                match local_cache.remove( &resp.0.to_string() ){
                    Some( r ) => {
                        debug!("Cache: removing `{}` from cache: expired.", &resp.0);
                        local_size = local_size - r.cached_response.http_content.len();
                    }
                    _ => {}
                }
            }
        });

        // Re-instate cache and size
        self.cache = local_cache;
        self.size = local_size;

    }

    pub fn get_size_mb( &self ) -> usize{
        self.size / (1024*1024)
    }

    pub fn drop( &mut self, url: &str ){
        match self.cache.remove( &url.to_string() ){
            Some( r ) => self.size = self.size - r.cached_response.http_content.len(),
            _ => {}
        }
    }

    // Get the response cached for this URL or None.
    pub fn get( &self, url: &str ) -> Option<Response>{
        match self.cache.get( url ){
            Some( resp ) => {
                if resp.is_expired(){
                    // It would make sense to delete the expired response from the cache,
                    // but this is a *read* method, and self is not mutable.
                    debug!("Cache: expired response to `{}`.", url);
                    None
                }else{
                    Some( resp.cached_response.clone() )
                }
            },
            _ => {
                info!("Cache: not in cache: `{}`", url);
                None
            }
        }
    }
}

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
    let pg_api_muscle_config = Arc::new(MuscleConfigCommon::new( &args[1] ));

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
    let db_pools: HashMap<String, Pool> = pg_api_muscle_config.contexts.values().map(|context | {
        let mut deadpool_config = Config::new();
        deadpool_config.dbname = Some(context.db.to_string());
        deadpool_config.user = Some(context.db_user.to_string());
        deadpool_config.password = Some(context.db_pass.to_string());
        deadpool_config.manager = Some(ManagerConfig { recycling_method: RecyclingMethod::Fast });

        ( context.prefix.to_owned(), deadpool_config.create_pool(NoTls).unwrap() )
    }).collect();

//    This (below) does not make sure that the timezone is set on all clients;
//    it may set the timezone on *recycled* clients, but when a new client is
//    initiated into the pool, GMT is set again. Waiting for a future version 
//    of tokio for this, cf.
//    <https://github.com/sfackler/rust-postgres/issues/147#event-4149833164>
//    cfg.manager = Some(ManagerConfig { recycling_method: RecyclingMethod::Custom(format!("set timezone='Europe/Berlin'")) });
//    Nor does this work:
//    cfg.options = Some(format!("-c timezone={}", conf.timezone.to_string()));
//    let pool = deadpool_config.create_pool(NoTls).unwrap();

    // adjust_timezone( &mut pool.get().await.unwrap(), "Europe/Berlin").await;
    // DEADPOOL END
    // -------------------------------------------------------

    // time_out specifies when to stop waiting for more
    // input from the socket
    let read_timeout = Duration::from_millis( pg_api_muscle_config.server_read_timeout_ms );
    let chunksize = pg_api_muscle_config.server_read_chunksize;
    let muscle_config = Arc::clone( &pg_api_muscle_config );

    // The restriction is only possible application-wide, not for 
    // individual contexts.
    // eine, die generell alle IPs begrenzt. Und eine, die erstmal nach dem 
    // Kontext schaut, und dann ggf. begrenzt? Oder ist das eine längerfristige 
    // Aufgabe, wenn man mal mit whitelists/blacklists/wildcards arbeiten möchte?
    let b_check_client_ip = !muscle_config.client_ip_allow.eq(&Ipv4Addr::new(0,0,0,0));

    let muscle_apis_a:HashMap<String, Arc<Mutex<API>>> = pg_api_muscle_config.contexts.values().map( | context | {
        (context.prefix.to_owned(), Arc::new(Mutex::new(
            API::new( &muscle_config.addr, 
                &context.token_name, 
                &context.pg_setvar_prefix, 
                &context.api_conf,
                context.use_eq_syntax_on_url_parameters
        ))))
    }).collect();

    let response_cache_base= ResponseCache::new();
    let response_cache = Arc::new( RwLock::new(response_cache_base) );

    let muscle_apis = Arc::new( muscle_apis_a );

    log_config_information(&muscle_config, &pg_api_muscle_config);

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
        // IST WIEDER IM Common-Config (19.10.21)
        // @todo-2021-10-3 Re-activate when client-ip is accessible again
//        let b_client_ip ... ?
        if b_check_client_ip {
            if !client_ip.parse::<Ipv4Addr>().unwrap().eq(&muscle_config.client_ip_allow){ 
                debug!("Request from >{}< ignored due to client_ip_allow restrictions set in initialisation file", client_ip);
                continue; 
            }
        }

        // Clone some objects things for the spawned thread:
        let spawned_apis = Arc::clone(&muscle_apis);
        let spawned_conf = Arc::clone( &pg_api_muscle_config );
        let spawned_pool = db_pools.clone();
        let spawned_response_cache = Arc::clone( &response_cache);
        //let spawned_response_cache = response_cache.clone();

        // Deal with the connection
        tokio::spawn(async move {

            // If the API is configured to listen for https: accept the TLS connection.
            // otherwise get the TcpStream
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

            // A little q&d: I need the prefix of the request at this point,
            // so I can choose the right API for the response-construction.
            let mut request = Request::new(&s_request, &client_ip,&spawned_conf.addr );

            // Hätte hier gerne ein Request-Objekt mit seinen Methoden, um
            // (1) schauen zu können, was das Prefix ist, und
            // (2) zu wissen, ob es ein shutdown/reload request ist.
            let prefix = match &request.get_prefix(){
                Some( prefix ) => prefix.to_owned(),
                _ => String::from( "#no_prefix" ).to_owned()
            };

            let mut is_known_prefix = false;
            let mut cached_r: Option<Response> = None;
            // NEU CACHE TEST 2021-10-23
            if !spawned_conf.contexts.get( &prefix ).is_none(){
                request.pg_service_prefix = spawned_conf.contexts.get( &prefix ).unwrap().pg_service_prefix.to_owned();
                
                let local_response_cache = spawned_response_cache.read().await;
                let opt_cached_response = local_response_cache.get ( &request.url );

//                if request.is_static(){
                    cached_r = match opt_cached_response{
                        Some( a ) => {
                            let re = a.clone();
                            info!("CACHE: Found this request in the cache...");
                            Some( re.to_owned() )
                        },
                        _ => None

                    };
//                };

                // Free the lock
                drop( local_response_cache );
            }
            let mut response ;
            if cached_r.is_none(){
                // NEU CACHE TEST 2021-10-23 ENDE
                response = match spawned_conf.contexts.get( &prefix ).is_none(){
                    true => Response::new_404(),
                    _ =>  {
                        is_known_prefix = true;
                        request.set_token_secret(&spawned_conf.contexts.get( &prefix ).unwrap().token_secret);
                        let mut api_ = spawned_apis.get( &prefix ).unwrap().lock().await;
                        let mut api = api_.borrow_mut();
                        api.set_request( &request );
                        Response::new( &mut api, &spawned_pool.get( &prefix ).unwrap(), &spawned_conf.contexts.get( &prefix ).unwrap() ).await
                    }
                };
            }else{
                info!("Sending response from the cache. ");
                response = cached_r.unwrap().clone();
            }

            // BRAUCHT IF NICHT IM CACHE...
            // NEU CACHE
            
            // THIS IF NEEDS STRUCTURE, BECAUSE ALSO USED ABOVE ... 
            // background is: is_static needs the prefix set.
            if !spawned_conf.contexts.get( &prefix ).is_none(){

            // Need a better determination of what to cache. Obviously
            // cacheing static requests excluively makes little sense.
//            if request.is_static(){
                let mut ttmp = spawned_response_cache.write().await;
                ttmp.add( request.url.clone(), response.clone()  );
//            }
            }

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
            if is_known_prefix && (spawned_apis.get( &prefix ).unwrap().lock().await.request).is_shutdown{ 
                info!("Shutting down on request.");
                exit(0);
            }
        });
    } // LOOP
}

fn log_config_information(muscle_config: &Arc<MuscleConfigCommon>, pg_api_muscle_config: &Arc<MuscleConfigCommon>) {
    info!("~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~");
    info!("Starting pg_api_muscle service");
    info!("Listening to port {}", muscle_config.port);
    info!("Https? {}", muscle_config.server_use_https);
    pg_api_muscle_config.contexts.values().for_each( | cx | {
        info!("================");
        info!("{}", cx.prefix);
        info!("================");
        info!("- db {}", cx.db);
        info!("- api {}", cx.api_conf);
        info!("- service-prefix {}", cx.pg_service_prefix);
        info!("");
        info!("- eq-syntax? {}", cx.use_eq_syntax_on_url_parameters);
    });
    info!("~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~");
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
        let mut r:Request = Request::new( "GET /path/to/this?a=1&b=ä", "::1", "127.0.0.1");
        r.pg_service_prefix="database".to_string();
        //let mut r:Request = Request::new( "/path/to/this?a=1&b=ä", "::1", "127.0.0.1", "", "static" );
        assert_eq!( r.get_query_parameter_value( "a" ),  Some("1") );
        assert_eq!( r.get_query_parameter_value( "b" ),  Some("ä") );
        assert_eq!( r.get_query_parameter_value( "c" ),  None );
        assert_eq!( r.get_payload_param( "c" ),  None );
        assert_eq!( r.get_prefix(  ).unwrap(),  "path" );
        assert_eq!( r.is_static(),  false );
    }
    #[test]
    fn simple1() {
        let mut r:Request = Request::new( "GET /path/to/this", "::1", "127.0.0.1");
        r.pg_service_prefix="database".to_string();
        assert_eq!( r.get_query_parameter_value( "a"),  None );
        assert_eq!( r.get_query_parameter_value( "c" ),  None );
        assert_eq!( r.get_payload_param( "c" ),  None );
        assert_eq!( r.get_prefix(  ).unwrap(),  "path" );
        assert_eq!( r.get_url_sans_prefix(  ),  "to/this" );
        assert_eq!( r.url,  "path/to/this" );
        assert_eq!( r.is_static(),  false );
    }

    #[test]
    fn s_payload() {
        let mut r:Request = Request::new( "path/to/this\n{\"this\":\"that\"}", "::1", "127.0.0.1");
        r.pg_service_prefix="database".to_string();
        assert_eq!( r.get_query_parameter_value( "a"),  None );
        assert_eq!( r.get_query_parameter_value( "c" ),  None );
        assert_eq!( r.get_payload_param( "this" ).unwrap().as_str(),  Some("that") );
        assert_eq!( r.is_static(),  false );

    }

    #[test]
    fn get_static() {
        let mut r:Request = Request::new( "/static/path/to/this\n{\"this\":\"that\"}", "::1", "127.0.0.1");
        r.pg_service_prefix="database".to_string();
        assert_eq!( r.is_static(),  true );
    }

    #[test]
    fn get_auth() {
        let r:Request = Request::new( "/static/path/to/this\n{\"this\":\"that\"}\nAuthorization: Bearer 1234&äß", "::1", "127.0.0.1");
        assert_eq!( r.get_auth(),  "1234&äß" );
//        assert_eq!( r.has_token(),  true );
    }

    #[test]
    fn get_auth_problematic_short() {
        let r:Request = Request::new( "/static/path/to/this\n{\"this\":\"that\"}\nAuthorization: Bearer", "::1", "127.0.0.1");
        assert_eq!( r.get_auth(),  "" );
//        assert_eq!( r.has_token(),  false );
    }

    #[test]
    fn get_auth_problematic_long() {
        let r:Request = Request::new( "/static/path/to/this\n{\"this\":\"that\"}\nAuthorization: Bearer 1234567890123456789012345678901234567890", "::1", "127.0.0.1");
        assert_eq!( r.get_auth(),  "1234567890123456789012345678901234567890" );
//        assert_eq!( r.has_token(),  true );
    }

    #[test]
    fn s_broken_payload() {
        let mut r:Request = Request::new( "path/to/this\n{\"this\":\"that\"", "::1", "127.0.0.1");
        r.pg_service_prefix = "database".to_string();
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
