use crate::config::MuscleConfigContext;
use std::{fmt::{self}};
use crate::Request;
use crate::request::RequestMethod;
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
    pub http_status_len_header: String,
    pub http_status: HttpStatus,
    pub content_type_header: String,
    pub http_content: Vec<u8>,
    pub is_static: bool
}

#[derive(Debug, PartialEq, Clone)]
pub enum HttpStatus{
    HTTP200,
    HTTP404,
    HTTP400,
    HTTP500
}

impl HttpStatus{
    pub fn as_string( status: &HttpStatus ) -> String{
        match status{
            HttpStatus::HTTP200 => "HTTP/1.1 200 OK".to_string(),
            HttpStatus::HTTP400 => "HTTP/1.1 400 BAD REQUEST".to_string(),
            HttpStatus::HTTP404 => "HTTP/1.1 404 NOT FOUND".to_string(),
            HttpStatus::HTTP500 => "HTTP/1.1 500 INTERNAL SERVER ERROR".to_string()
        }
    }

    fn is_error( &self ) -> bool{
        match self{
            HttpStatus::HTTP200 => false,
            HttpStatus::HTTP400 => true,
            HttpStatus::HTTP404 => true,
            HttpStatus::HTTP500 => true
        }
    }
}

impl Default for HttpStatus {
    fn default() -> Self { HttpStatus::HTTP500 }
}

impl fmt::Display for HttpStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
      write!(f, "{}", HttpStatus::as_string( self) )
    }
}

impl Response{

    const CONTENT_TYPE_JSON: &'static str = "application/json;charset=UTF-8";
//    const CONTENT_TYPE_HTML: &'static str = "text/html;charset=UTF-8";

    pub fn new_404( ) -> Self{
        let header = "Content-Type: text/html;charset=UTF-8\r\n".to_string();
        let response =  ( HttpStatus::HTTP404, String::from("<html><body>Not found</body></html>").as_bytes().to_vec());

        Self{
            http_status_len_header: format!( "{}\r\nContent-Length: {}\r\n{}\r\n", response.0, response.1.len(), header),
            http_status: response.0,
            http_content: response.1,
            content_type_header: header,
            is_static: true
        }
    }
    /* Constructor */
    pub async fn new( api: &mut API, client: &Pool, conf: &MuscleConfigContext ) -> Self {

        info!("Handling >{}< request for >{}< from >{}< with params >{}< and (abbrev.) payload >{}<", 
            Request::get_method_as_str( api.request.method ), 
            api.request.url, 
            api.request.ip_address, 
            api.request.q_parms, 
            api.request.p_parms.chars().take(80).collect::<String>());
        
        let mut db_response = match api.request.method{
            RequestMethod::GET =>  Response::handle_get( api, client, &conf).await,
            RequestMethod::DELETE => Response::handle_delete( api, client ).await,
            RequestMethod::POST => Response::handle_post( api, client).await,
            RequestMethod::PATCH => Response::handle_patch( api, client).await,
            _ => ( HttpStatus::HTTP404, b"Method not implemented".to_vec() )
        };

//        let content_type = Response::CONTENT_TYPE_JSON;
        //@TODO have variable content-type header
        let content_type_header = "Access-Control-Allow-Origin: *";

        // @TODO HEader konfigurierbar machen:
        // (1) JSON macht sicher Sinn,
        // (2) Access-Control etc. erlaubt Anfragen von Skripts anderer Seiten (man kann da
        // (3) Mime-Guess muss expandiert werden auf andere Typen als nur png.
        // spezifizieren!)
        let header = match api.request.is_static() {
            true => Response::get_mime_guess( &api.request.url ),
            _ => format!("{}\r\n{}\r\n", Response::CONTENT_TYPE_JSON, content_type_header) // "Content-Type: application/json;charset=UTF-8\r\nAccess-Control-Allow-Origin: *\r\n".to_string()
        };

        // Error response is quite variable, depending on 
        // whether this was a static request or not, and 
        // how much information about the error is supposed
        // to become transparent in the response
        if db_response.0.is_error(){

            // static requests already are taken care
            // of in handle_get
            if !api.request.is_static(){ 
            
                // if db requests are allowed to pass through all available
                // information (i.e. dynamic_err=default), we're good
                if !conf.dynamic_err.eq("default"){

                    if conf.dynamic_err.starts_with('{'){
                        db_response.1 = conf.dynamic_err.as_bytes().to_vec();
                    }else{
                        // dynamic request with static response file:
                        db_response.1 = match File::open(&conf.dynamic_err){
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
                } // not 'default'
            } // not static
        }

        Self{
            http_status_len_header: format!( "{}\r\nContent-Length: {}\r\n{}\r\n", db_response.0, db_response.1.len(), header),
            http_status: db_response.0,
            http_content: db_response.1,
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

    async fn handle_patch( api: &mut API, client: &Pool ) -> (HttpStatus, Vec<u8>){

        match &api.get_request_deviation( )[..]{

            // Request does not deviate from api:
            "" => match get_db_response( client, api ).await{

                Ok( db_response ) => (HttpStatus::HTTP200, db_response.as_bytes().to_vec()),
                Err( e ) => {error!("...db problem on PATCH: {}", e);
                    (HttpStatus::HTTP400, serde_json::to_string( 
                            &APIError{ message: e.to_string(), hint: "No hint".to_string()}).unwrap().as_bytes().to_vec()) 
                } 
            },

            // Request DOES deviate from api, let's produce an error
            deviation => {error!("... bad PATCH request: `{}`.", deviation); 
                (HttpStatus::HTTP400, 
                 serde_json::to_string( &APIError{ message: deviation.to_string(), hint: "No hint".to_string()}).unwrap().as_bytes().to_vec()) } 
        }
    }

    async fn handle_delete( api: &mut API, client: &Pool ) -> (HttpStatus, Vec<u8>){

        match &api.get_request_deviation()[..]{

            // Request does not deviate from api:
            "" => match get_db_response( client, api ).await{

                Ok( db_response ) => (HttpStatus::HTTP200, db_response.as_bytes().to_vec()),
                Err( e ) => {error!("...db problem on DELETE: {}", e);
                    (HttpStatus::HTTP400, serde_json::to_string( 
                            &APIError{ message: e.to_string(), hint: "No hint".to_string()}).unwrap().as_bytes().to_vec())
                } 
            },

            // Request DOES deviate from api:
            deviation => {error!("... bad DELETE request: `{}`.", deviation); 
                (HttpStatus::HTTP400, 
                 serde_json::to_string( &APIError{ message: deviation.to_string(), hint: "No hint".to_string()}).unwrap().as_bytes().to_vec()) } 
        }
    }

    async fn handle_post( api: &mut API, client: &Pool ) -> (HttpStatus, Vec<u8>){

       match &api.get_request_deviation()[..]{

            // Request does not deviate from api
            "" => match get_db_response( client, api ).await{

                Ok( db_response ) => (HttpStatus::HTTP200, db_response.as_bytes().to_vec()),
                Err( e ) => {error!("...db problem on POST: {}", e);
                    (HttpStatus::HTTP400, serde_json::to_string( 
                            &APIError{ message: e.to_string(), hint: "No hint".to_string()}).unwrap().as_bytes().to_vec()) 
                } 
            },

            // Request DOES deviate from api:
            deviation => {error!("... bad POST request: `{}`.", deviation); 
                (HttpStatus::HTTP400, 
                 serde_json::to_string( 
                     &APIError{ message: deviation.to_string(), hint: "No hint".to_string()}).unwrap().as_bytes().to_vec()) } 
        }
    }

    /// Returns .1 status and headers, .2 content
    async fn handle_get( api: &mut API, client: &Pool, conf: &MuscleConfigContext ) -> (HttpStatus, Vec<u8>){

        // ========================================================================
        // Static request, send file
        if api.request.is_static() {
            // @TODO needs thinking over: what does this concatenation do? Nothing? But might be useful for an independant static files thing?
//            let mut f_path = conf.static_files_folder.to_owned() + &api.request.get_url_sans_prefix().chars().skip(conf.static_files_folder.to_owned().len()).collect::<String>().to_string();
//            let mut f_path = api.request.get_url_sans_prefix().chars().skip(conf.static_files_folder.to_owned().len()).collect::<String>().to_string();
            let mut f_path = api.request.url.to_owned();
//            let mut f_path = conf.static_files_folder.to_owned() + &api.request.url.chars().skip(conf.static_files_folder.to_owned().len()).collect::<String>().to_string();
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
                ( HttpStatus::HTTP404, page )
            }else{
                ( HttpStatus::HTTP200, page )
            }

        }else{
            // ========================================================================
            // Dynamic request, convert to DB response
            match &api.get_request_deviation()[..]{

                // Request does not deviate from api
                "" => match get_db_response( client, api ).await{

                    Ok( db_response ) => ( HttpStatus::HTTP200, db_response.as_bytes().to_vec() ),

                    Err( e ) => {info!("...db problem on GET: {}", e);
                        ( HttpStatus::HTTP400, format!("{} ", serde_json::to_string( 
                                &APIError{ message: e.to_string(), hint: "No hint".to_string()}).unwrap()).as_bytes().to_vec())
                    }
                },

                // Request DOES deviate from api:
                deviation => {error!("... bad GET request: `{}`.", deviation); 
                    ( HttpStatus::HTTP400, 
                      serde_json::to_string( 
                          &APIError{ message: deviation.to_string(), hint: "No hint".to_string() } 
                ).unwrap().as_bytes().to_vec())}
            }
        }
    }
}