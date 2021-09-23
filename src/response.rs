use crate::MuscleConfig;
use crate::Request;
use crate::RequestMethod;
use deadpool_postgres::{Pool };
use crate::db::get_db_response;
use crate::API;
use log::{error, info};
use std::io::prelude::*; // needed for read_do_end
use std::fs::File;

#[derive(Serialize, Deserialize, Debug)]
struct APIError {
    message: String,
    hint: String
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Response {
    pub http_status: String,
    pub content_type_header: String,
    pub http_content: Vec<u8>,
    is_static: bool
}

impl Response{

    const HTTP_404: &'static str = "HTTP/1.1 404 NOT FOUND";
    const HTTP_400: &'static str = "HTTP/1.1 400 BAD REQUEST";
    const HTTP_200: &'static str = "HTTP/1.1 200 OK";

    const CONTENT_TYPE_JSON: &'static str = "application/json;charset=UTF-8";
    const CONTENT_TYPE_HTML: &'static str = "text/html;charset=UTF-8";

    fn is_error( HTTP_status: &str ) -> bool{
        match HTTP_status{
            Response::HTTP_404 => true,
            Response::HTTP_400 => true,
            Response::HTTP_200 => false,
            _ => true
        }
    }

    /* Constructor */
    pub async fn new( api: &mut API, client: &Pool, conf: &MuscleConfig ) -> Self {

        info!("Handling >{}< request for >{}< from >{}< with params >{}< and (abbrev.) payload >{}<", 
            Request::get_method_as_str( api.request.method ), 
            api.request.url, 
            api.request.ip_address, 
            api.request.q_parms, 
            api.request.p_parms.chars().take(80).collect::<String>());
        
        let mut s_resp = match api.request.method{
            RequestMethod::GET =>  Response::handle_get( api, client, &conf).await,
            RequestMethod::DELETE => Response::handle_delete( api, client ).await,
            RequestMethod::POST => Response::handle_post( api, client).await,
            RequestMethod::PATCH => Response::handle_patch( api, client).await,
            _ => ( Response::HTTP_404.to_string(), b"Method not implemented".to_vec() )
        };

        //
//        let content_type = Response::CONTENT_TYPE_JSON;
        //@TODO have variable content-type header
        let mut content_type_header = "Access-Control-Allow-Origin: *";

        // @TODO HEader konfigurierbar machen:
        // (1) JSON macht sicher Sinn,
        // (2) Access-Control etc. erlaubt Anfragen von Skripts anderer Seiten (man kann da
        // (3) Mime-Guess muss expandiert werden auf andere Typen als nur png.
        // spezifizieren!)
        let mut header = match api.request.is_static() {
            true => Response::get_mime_guess( &api.request.url ),
            _ => format!("{}\r\n{}\r\n", Response::CONTENT_TYPE_JSON, content_type_header) // "Content-Type: application/json;charset=UTF-8\r\nAccess-Control-Allow-Origin: *\r\n".to_string()
        };

        if Response::is_error(&s_resp.0[..]){

            // static requests already are taken care
            // of in handle_get
            if !api.request.is_static(){ 
            
                // if db requests are allowed to pass through all available
                // information (i.e. dynamic_err=default), we're good
                if !conf.dynamic_err.eq("default"){

                    if conf.dynamic_err.starts_with('{'){
                        s_resp.1 = conf.dynamic_err.as_bytes().to_vec();
                    }else{
                        // dynamic request with static response file:
                        s_resp.1 = match File::open(&conf.dynamic_err){
                                Ok (mut file) => {
                                    let mut buffer = Vec::new();
                                    match file.read_to_end(&mut buffer){
                                        Ok( _ ) => buffer,
                                        _ => { error!("Configuration error: dynamic error response file {} cannot be read. Sending empty response", conf.dynamic_err); vec![] }
                                    }
                                },
                                _ => { error!("Configuration error: dynamic error response file {} not found. Sending empty response", conf.dynamic_err); vec![] }
                            }
                    }// else
                }

            }

        }

        // Error handling

        Self{
            http_status: s_resp.0,
            http_content: s_resp.1,
            content_type_header: header,
            is_static: api.request.is_static()
        }
    }

    // Extend with .js, .css, jpg, jpeg, mp3, mpeg
    fn get_mime_guess( url:&String ) -> String{
        
        //Binary ... look at this here: https://docs.rs/base64/0.13.0/base64/
        match url.ends_with( ".png" ){
            true => "Content-Type: image/png".to_string(),
            _ => "".to_string()
        }
    }

    /// Returns .1 status and headers, .2 content, .3 flag indicating if this was a request to a static resource
    pub fn get_response( self ) -> (String, Vec<u8>, bool){
//        if !self.dyn_response.eq( "default" ){ self.http_content = self.dyn_response} //ß
        (format!( "{}\r\nContent-Length: {}\r\n{}\r\n", self.http_status, self.http_content.len(), self.content_type_header), self.http_content, self.is_static)
// the following line works ... 
//        (format!( "{}\r\n{}\r\n", self.http_status, self.content_type_header), self.http_content, self.is_static)
    }

    /// Returns .1 status and headers, .2 content
    async fn handle_patch( api: &mut API, client: &Pool ) -> (String, Vec<u8>){

        match &api.get_request_deviation( )[..]{

            // Request does not deviate from api:
            "" => match get_db_response( client, api ).await{

                Ok( s ) => (Response::HTTP_200.to_string(), s.as_bytes().to_vec()),
                Err( e ) => {error!("...db problem on PATCH: {}", e);
                    (Response::HTTP_400.to_string(), serde_json::to_string( 
                            &APIError{ message: e.to_string(), hint: "No hint".to_string()}).unwrap().as_bytes().to_vec()) 
                } 
            },

            // Request DOES deviate from api, let's produce an error
            x => {error!("... bad PATCH request: `{}`.", x); 
                (Response::HTTP_400.to_string(), 
                 serde_json::to_string( &APIError{ message: x.to_string(), hint: "No hint".to_string()}).unwrap().as_bytes().to_vec()) } 
        }
    }

    /// Returns .1 status and headers, .2 content
    async fn handle_delete( api: &mut API, client: &Pool ) -> (String, Vec<u8>){

        match &api.get_request_deviation()[..]{

            // Request does not deviate from api:
            "" => match get_db_response( client, api ).await{

                Ok( s ) => (Response::HTTP_200.to_string(), s.as_bytes().to_vec()),
                Err( e ) => {error!("...db problem on DELETE: {}", e);
                    (Response::HTTP_400.to_string(), serde_json::to_string( 
                            &APIError{ message: e.to_string(), hint: "No hint".to_string()}).unwrap().as_bytes().to_vec())
                } 
            },

            // Request DOES deviate from api:
            x => {error!("... bad DELETE request: `{}`.", x); 
                (Response::HTTP_400.to_string(), 
                 serde_json::to_string( &APIError{ message: x.to_string(), hint: "No hint".to_string()}).unwrap().as_bytes().to_vec()) } 
        }
    }

    /// Returns .1 status and headers, .2 content
    async fn handle_post( api: &mut API, client: &Pool ) -> (String, Vec<u8>){

       match &api.get_request_deviation()[..]{

            // Request does not deviate from api
            "" => match get_db_response( client, api ).await{

                Ok( s ) => (Response::HTTP_200.to_string(), s.as_bytes().to_vec()),
                Err( e ) => {error!("...db problem on POST: {}", e);
                    (Response::HTTP_400.to_string(), serde_json::to_string( 
                            &APIError{ message: e.to_string(), hint: "No hint".to_string()}).unwrap().as_bytes().to_vec()) 
                } 
            },

            // Request DOES deviate from api:
            x => {error!("... bad POST request: `{}`.", x); 
                (Response::HTTP_400.to_string(), 
                 serde_json::to_string( 
                     &APIError{ message: x.to_string(), hint: "No hint".to_string()}).unwrap().as_bytes().to_vec()) } 
        }
    }

    /// Returns .1 status and headers, .2 content
    async fn handle_get( api: &mut API, client: &Pool, conf: &MuscleConfig ) -> (String, Vec<u8>){

        // ========================================================================
        // Static request, send file
        if api.request.is_static() {
            let mut f_path = "static/".to_owned() + &api.request.url.chars().skip(7).collect::<String>().to_string();
            let msg_not_found = "Sorry, requested ressource not found. ".as_bytes().to_vec();
            let mut b_is_404 = false;

            // If a folder is requested, look for an index file (as 
            // configured in muscle.ini)
            if f_path.ends_with('/'){ f_path.push_str(&conf.index_file[..]); }

            // Binary ... look at this here: https://docs.rs/base64/0.13.0/base64/
            // https://stackoverflow.com/questions/57628633/how-to-properly-format-in-rust-an-http-response-with-a-media-file
            
            // Open and read file, and respond with error messages as 
            // configured if the file cannot be opened or not be read. 
            // (Error response is either the default "Sorry, not found" msg 
            //  from above, or a configured file)
            let page:Vec<u8> = match File::open(f_path){
                    Ok (mut file) => {
                        let mut buffer = Vec::new();
                        match file.read_to_end(&mut buffer){
                            Ok( _ ) => buffer,
                            _ => { b_is_404 = true; msg_not_found }
                        }
                    },
                    _ => {
                        b_is_404 = true;
                        if conf.static_404_default.eq("none") { msg_not_found }
                        else{
                            match File::open(&conf.static_404_default){
                                Ok (mut file) => {
                                    let mut buffer = Vec::new();
                                    match file.read_to_end(&mut buffer){
                                        Ok( _ ) => buffer,
                                        _ => {error!("Cannot read default static 404 file >{}<; check configuration file", &conf.static_404_default); msg_not_found }
                                    }
                                }
                                Err(_) => {error!("Cannot find default static 404 file >{}<; check configuration file", &conf.static_404_default); msg_not_found }
                            }

                        }
                    }
            };

            if b_is_404 { 
//                ( Response::HTTP_404.to_string(), msg_not_found.as_bytes().to_vec() )
                ( Response::HTTP_404.to_string(), page )
            }else{
                ( Response::HTTP_200.to_string(), page )
            }

        }else{
            // ========================================================================
            // Dynamic request, convert to DB response
            match &api.get_request_deviation()[..]{

                // Request does not deviate from api
                "" => match get_db_response( client, api ).await{

                    Ok( s ) => ( Response::HTTP_200.to_string(), s.as_bytes().to_vec() ),

                    Err( e ) => {info!("...db problem on GET: {}", e);
                        ( Response::HTTP_400.to_string() ,format!("{} ", serde_json::to_string( 
                                &APIError{ message: e.to_string(), hint: "No hint".to_string()}).unwrap()).as_bytes().to_vec())
                    }
                },

                // Request DOES deviate from api:
                x => {error!("... bad GET request: `{}`.", x); 
                    ( Response::HTTP_400.to_string(), 
                      serde_json::to_string( 
                          &APIError{ message: x.to_string(), hint: "No hint".to_string() } 
                      ).unwrap().as_bytes().to_vec())}
            }
        }
    }
}
