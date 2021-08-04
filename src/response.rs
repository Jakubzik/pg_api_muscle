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

    /* Constructor */
    pub async fn new( api: &mut API, client: &Pool ) -> Self {

        info!("Handling >{}< request for >{}< from >{}< with params >{}< and (abbrev.) payload >{}<", 
            Request::get_method_as_str( api.request.method ), 
            api.request.url, 
            api.request.ip_address, 
            api.request.q_parms, 
            api.request.p_parms.chars().take(80).collect::<String>());
        
        let s_resp = match api.request.method{
            RequestMethod::GET =>  Response::handle_get( api, client).await,
            RequestMethod::DELETE => Response::handle_delete( api, client ).await,
            RequestMethod::POST => Response::handle_post( api, client).await,
            RequestMethod::PATCH => Response::handle_patch( api, client).await,
            _ => ( Response::HTTP_404.to_string(), b"Method not implemented".to_vec() )
        };

        // @TODO HEader konfigurierbar machen:
        // (1) JSON macht sicher Sinn,
        // (2) Access-Control etc. erlaubt Anfragen von Skripts anderer Seiten (man kann da
        // (3) Mime-Guess muss expandiert werden auf andere Typen als nur png.
        // spezifizieren!)
        let header = match api.request.is_static() {
            true => Response::get_mime_guess( &api.request.url ),
            _ => "Content-Type: application/json;charset=UTF-8\r\nAccess-Control-Allow-Origin: *\r\n".to_string()
        };

        Self{
            http_status: s_resp.0,
            http_content: s_resp.1,
            content_type_header: header,
            is_static: api.request.is_static()
        }
    }

    fn get_mime_guess( url:&String ) -> String{
        
        //Binary ... look at this here: https://docs.rs/base64/0.13.0/base64/
        match url.ends_with( ".png" ){
            true => "Content-Type: image/png".to_string(),
            _ => "".to_string()
        }
    }

    /// Returns .1 status and headers, .2 content, .3 flag indicating if this was a request to a static resource
    pub fn get_response( self ) -> (String, Vec<u8>, bool){
        (format!( "{}\r\nContent-Length: {}\r\n{}\r\n", self.http_status, self.http_content.len(), self.content_type_header), self.http_content, self.is_static)
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
    async fn handle_get( api: &mut API, client: &Pool ) -> (String, Vec<u8>){

        // ========================================================================
        // Static request, send file
        if api.request.is_static() {
            let f_path = "static/".to_owned() + &api.request.url.chars().skip(7).collect::<String>().to_string();
            let msg_not_found = "Sorry, requested ressource not found".to_string();

            //Binary ... look at this here: https://docs.rs/base64/0.13.0/base64/
            //https://stackoverflow.com/questions/57628633/how-to-properly-format-in-rust-an-http-response-with-a-media-file
            let page:Vec<u8> = match File::open(f_path){
                    Ok (mut file) => {
                        let mut buffer = Vec::new();
                        match file.read_to_end(&mut buffer){
                            Ok( _ ) => buffer,
                            _ => b"Error reading file.".to_vec()
                        }
                    },
                    _ => msg_not_found.to_string().as_bytes().to_vec()
            };

            if page == msg_not_found.as_bytes().to_vec() { 
                ( Response::HTTP_404.to_string(), msg_not_found.as_bytes().to_vec() )
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
                        ( Response::HTTP_400.to_string() ,serde_json::to_string( 
                                &APIError{ message: e.to_string(), hint: "No hint".to_string()}).unwrap().as_bytes().to_vec()) 
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
