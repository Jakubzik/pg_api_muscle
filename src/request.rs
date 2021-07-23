use log::{error, info};
use jwt_simple::prelude::*;
use serde_json::Value;
use crate::RequestMethod;
use crate::Authentication;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Request {
    pub url: String,
    query_is_read: bool,
    pub q_parms: String,
    pub p_parms: String,
    payload_is_read: bool,
    query_params: Vec<(String,String)>,
    content_type: String,
    authorization: String,
    pub auth_claim: Option<Value>,
    pub api_needs_auth: Authentication,
    pub token_secret: String,
    pub method: RequestMethod,
    pub method_reroute: RequestMethod,  // Do I need this? @TODO ... For POST to SQL procedures that need GET syntax from the response
    pub ip_address: String,
    payload: Value
}


impl Request{

    /* Constructor */

    // TODO
    // - Beim ersten Aufruf statisches Feld query_p_vector setzen (performance)
    // - vllt. ähnlich den Payload nur auf Anfrage auswerten
    // - wenn die Methoden .get_query_param und .get_payload_param stehen, guard umschreiben
    //

    /// Constructor for a Request struct that facilitates access
    /// to both query parameters and a json payload
    ///
    /// # Arguments
    ///
    /// * `s_req` - A String slice containing the HTTP request
    ///
    /// * `ip_address` - A String slice containing the client IP address
    ///
    /// Requests for static ressources must be GET and relative to 
    /// ./static/...
    ///
    /// Requests for function calls in postgrest (i.e. a POSTed payload with GET SQL)
    /// have x-query-syntax-of-method=GET
    /// 
    /// panics! 
    ///
    /// A request for shutdown can be sent as DELETE for pg_api_muscle:knockout
    ///
    /// ```
    /// curl --request DELETE "https://localhost:8443/pg_api_muscle:knockout"
    /// ```
    ///
    /// The service will then panic and shut down.
    ///
    ///
    /// # Example
    ///
    /// ```
    /// for stream in listener.incoming() {
    ///    stream.read( &mut buffer )?;
    ///    ...
    ///    let s_req = String::from_utf8_lossy( &buffer[..] );
    /// 
    ///   let s_ip_addr_client = match stream.get_ref().peer_addr(){
    ///     Ok( addr ) => addr.ip().to_string(),
    ///     Err( _e ) => String::from("ip unknown")
    ///   };
    ///   let request = &mut Request::new( &s_req, &s_ip_addr_client );
    ///   ...
    /// }
    /// ```
    pub fn new( s_req: &str, s_ip_addr_client: &str, token_secret: &str ) -> Self {

        // -----------------------------------------------------
        // Stream starts e.g. with "GET /path/to/foo?whater=1
        // Last line is payload. Analyze:
        let s_first_line = s_req.lines().next().unwrap();
        let s_uri = Request::get_uri( &s_first_line );
        let url_plus_par: (&str, &str) = Request::get_url_plus_parms( &s_uri );
        let ct_payload_auth: (&str, &str, &str) = Request::get_content_payload_auth( &s_req );

        if Request::get_method( &s_first_line ) == RequestMethod::SHUTDOWN {panic!("Shutdown requested");}
        
        let claims = Request::get_auth_claims( ct_payload_auth.2.to_string(), token_secret.to_string() );
        Self{
            payload_is_read: false,
            query_is_read: false,
            q_parms: url_plus_par.1.to_string(),
            p_parms: ct_payload_auth.1.to_string(),
            url: url_plus_par.0.to_string(),
            query_params: vec![],
            method: Request::get_method( &s_first_line ),
            method_reroute: RequestMethod::UNKNOWN,
            content_type: ct_payload_auth.0.to_string(),
            authorization: ct_payload_auth.2.to_string(),
            auth_claim: claims,
            api_needs_auth: Authentication::UNKNOWN,
            token_secret: token_secret.to_string(),
            ip_address: s_ip_addr_client.to_string(),
            payload: Value::Null
        }
    }

    /// This request's query parameters as Vector<(Name, Value)>
    fn get_query_params_as_vector( &mut self ) -> &Vec<(String, String)> {
        if !self.query_is_read {
            self.query_params = serde_urlencoded::from_str::<Vec<(String, String)>>( &self.q_parms ).unwrap();
            self.query_is_read = true;
        }
        &self.query_params
    }

    /// This request's payload parameters as Vector<(Name, Value)>
    fn get_payload_params_as_value( &mut self ) -> &Value{
        if !self.payload_is_read {
            self.payload = match serde_json::from_str( &self.p_parms.to_owned() ){
                Ok( x ) => x,
                Err( e ) => {info!("Cannot get a payload, setting payload to empty: {}", e); Value::Null}
            };
            self.payload_is_read = true;
        }
        &self.payload
    }
    
    /// Get value of a parameter in this request's query
    ///
    /// # Example
    ///
    /// ```
    /// // Assuming ./page?this=foo&that=bar was called
    /// assert_eq( request.get_query_param( "this", "foo" );
    /// assert_eq( request.get_query_param( "that", "bar" );
    /// ```
    pub fn get_query_param( &mut self, s_name: &str ) -> Option<&str>{
        match self.get_query_params_as_vector().into_iter().find( | &p | { p.0 == s_name } ){
            Some( x ) => Some( &x.1 ),
            None => None
        }
    }
    
    /// Get value of a parameter in this request's json payload
    pub fn get_payload_param( &mut self, s_name: &str ) -> Option<&Value>{
        self.get_payload_params_as_value().get( s_name )
    }

    /// Is this a request for a static page?
    pub fn is_static( &self ) -> bool {
        self.url.starts_with("static/")
    }
    
    /// Authentication Token
    pub fn get_auth( &self ) -> &String {
        &self.authorization
    }

    pub fn has_valid_auth( &self ) -> bool{
        match self.auth_claim{
            Some ( _ ) => true,
            None => false
        }
    }

    fn get_auth_claims( s_auth: String, s_token_secret: String ) -> Option<Value> {
//        let key = HS256Key::from_bytes(b"5JkCkNsRw7Iww16OILugtNso8UCzXluo");
        let key = HS256Key::from_bytes( s_token_secret.as_bytes() );
        match key.verify_token::<Value>(&s_auth, None){
            Ok( cl ) => {info!("bearer verified, {:?}, {:?}, {:?}", 
                cl.custom, cl.issued_at, cl.expires_at); 
                // @TODO: issued_at liefert None, expires_at liefert Some(Duration(6958153388127158272))
                // https://docs.rs/jwt-simple/0.10.0/jwt_simple/claims/struct.JWTClaims.html
                // Abgelaufenes Token sollte hier "bearer failed" verursachen.
                Some ( serde_json::to_value( cl ).unwrap() )
            },
            Err( _e ) => {info!("bearer failed: {}", &s_auth); None}
        }
    }

    // -------------------------------------------------------------------------------
    // private static helper methods

    /// Static method: extract method from HTTP String
    fn get_method( s_line: &str )-> RequestMethod {

        let s = s_line.split("/").collect::<Vec<&str>>()[0];

        if s_line.to_lowercase().starts_with("delete /pg_api_muscle:knockout") { return RequestMethod::SHUTDOWN }

        match &s.to_lowercase().trim()[..]{
            "get" => RequestMethod::GET,
            "post" => RequestMethod::POST,
            "patch" => RequestMethod::PATCH,
            "delete" => RequestMethod::DELETE,
            x => {
                error!("HEADS UP: request with unknown HTTP-method: '{}'", x);
                RequestMethod::UNKNOWN
            }
        }
    }

    /// Static method: extract method from HTTP String
//    fn get_method( s_line: &str )-> u8 {
//        let s = s_line.split("/").collect::<Vec<&str>>()[0];
//
//        if s_line.to_lowercase().starts_with("delete /pg_api_muscle:knockout") { 
//            return Request::METHOD_SHUTDOWN_REQUEST; 
//        }
//
//        match &s.to_lowercase().trim()[..]{
//            "get" => Request::METHOD_GET,
//            "post" => Request::METHOD_POST,
//            "patch" => Request::METHOD_PATCH,
//            "delete" => Request::METHOD_DELETE,
//            x => {
//                error!("HEADS UP: request with unknown HTTP-method: '{}'", x);
//                Request::METHOD_UNKNOWN
//            }
//        }
//    }

    /// Static method: extract uri (including parameters) from HTTP request
    fn get_uri( s_first_line: &str ) -> String{
        let s_uri = match s_first_line.splitn(2, '/').last(){
            Some( uri ) => uri.to_string(),
            _ => {error!("Cannot parse into URI: {}", s_first_line); 
                format!("Cannot parse URI of request: {}", s_first_line)}
        };

        // Remove "_HTTP/1.1"
        match s_uri.split(' ').next(){
            Some( uri ) => uri.to_string(),
            _ => format!("Cannot parse URI of request: {}", s_first_line)
        }
    }

    /// Static method: separate url from query parameters
    fn get_url_plus_parms( s_uri: &str ) -> (&str, &str){
        let mut s_qparms = "";
        let s_url;
        if s_uri.contains( "?" ){
            let v_tmp: Vec<&str> = s_uri.split("?").collect();
            s_url = v_tmp[ 0 ];
            s_qparms = v_tmp[ 1 ];
        }else{
            s_url = &s_uri;
        }
        ( s_url, s_qparms )
    }

    fn crop_letters(s: &str, pos: usize) -> &str {
        match s.char_indices().skip(pos).next() {
            Some((pos, _)) => &s[pos..],
            None => "",
        }
    }

    /// Static method: extract content-type and payload from request.
    fn get_content_payload_auth( s_req: &str ) -> (&str, &str, &str){
        // -----------------------------------------------------
        // Payload is in the last line: go through lines 
        // and extract content-type as well
        let mut s_last = "";
        let mut s_content_type = "";
        let mut s_authorization = "";
        for line in s_req.lines(){ 
            if line.starts_with( "Content-Type: " ){ s_content_type = line.split(":").last().unwrap().trim_start();}
            if line.starts_with( "Authorization: Bearer " ){ 
                s_authorization = Request::crop_letters( &line, 22 ); // 22 = "Authorization Bearer ".length
            }
            s_last = line; 
        }

        // Remove UTF-8 Null characters
        s_last = s_last.trim_matches( char::from(0) );

        ( s_content_type, s_last, s_authorization )
    }

    pub fn get_method_as_str( method: RequestMethod ) -> &'static str{
        match method{
            RequestMethod::GET => "get",
            RequestMethod::POST => "post",
            RequestMethod::POSTasGET => "post->get",
            RequestMethod::PATCH => "patch",
            RequestMethod::DELETE => "delete",
            _ => "unknown"
        }
    }

//    pub fn get_method_as_str( method: u8 ) -> &'static str{
//        match method{
//            Request::METHOD_GET => "get",
//            Request::METHOD_POST => "post",
//            Request::METHOD_POST_RESET_TO_GET => "post->get",
//            Request::METHOD_PATCH => "patch",
//            Request::METHOD_DELETE => "delete",
//            _ => "unknown" 
//        }
//    }

}
#[cfg(test)]
mod test_get_query{
    use super::*;
    #[test]
    fn test_get_url_plus_parms() {
        assert_eq!( Request::get_url_plus_parms("Whatever?this=that&a=b").0, "Whatever" );
        assert_eq!( Request::get_url_plus_parms("Whatever?this=that&a=b").1, "this=that&a=b" );

    }
}
