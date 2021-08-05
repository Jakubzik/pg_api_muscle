// #[macro_use]
use crate::Request;
use crate::RequestMethod;
use crate::Authentication;
use crate::CheckedParam;
use crate::UnCheckedParam;
use crate::S_EMPTY;
use crate::ParamVal;
use crate::APIParam;
use crate::Schema;

use std::{fs::File, io::BufReader};
use std::{
    convert::TryInto
};
use log::{debug, error, info};

//#[json]
use serde_json::Value;

//#[derive(Debug, PartialEq)]
pub struct API {
    pub checked_query_parameters: Vec<CheckedParam>,
    problems_query_parameters: String,
    checked_query_params_read: bool,
    pub checked_post_parameters: Vec<CheckedParam>,
    problems_post_parameters: String,
    checked_post_params_read: bool,
    pub request: Request,
    token_name: String,
    pg_setvar_prefix: String,
    pub pg_set: String,
    request_set: bool,
    routing_json: Value,
    routing_file_path: String,
    routing_file_read: bool,
    pub local_ip_address: String // corresponds to muscle.ini, no checks made. Needed for shutdown and reload requests
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ClaimItem{
    name: String,
    value: Option<String>,
    checkval: Option<String>,
    pg_set_as: Option<String>
}

impl API{

    const API_PATHS: &'static str = "paths";
    const API_QUERY: &'static str = "operationId";

    const TYPE_STRING: &'static str = "string";
    const TYPE_INTEGER: &'static str = "integer";
    const TYPE_BIGINT: &'static str = "bigint";
    const TYPE_BOOLEAN: &'static str = "boolean";
    const TYPE_NUMBER: &'static str = "number";
    
    const SUPERFLUOUS_PARAMETER: &'static str = "superfluous_parm_not_present";

    const PARAM_TYPE_PAYLOAD:u8 = 0;
    const PARAM_TYPE_QUERY:u8 = 1;

    /// API provides functions to check the request
    /// against the JSON route (defined 
    /// through OpenAPI)
    ///
    /// The API struct remains initialized with the API. In order to check 
    /// a new request, call .set_request.
    pub fn new( addr: &str, pg_token_name: &str, pg_setvar_prefix: &str, s_routing_file: &str ) -> Self{

        API{
            checked_query_parameters: vec![],
            problems_query_parameters: S_EMPTY,
            checked_query_params_read: false,
            checked_post_parameters: vec![],
            problems_post_parameters: S_EMPTY,
            checked_post_params_read: false,
            token_name: pg_token_name.to_string(),
            pg_setvar_prefix: pg_setvar_prefix.to_string(),
            pg_set: "".to_string(),
            routing_file_path: s_routing_file.to_string(),
            routing_file_read: false,
            routing_json: serde_json::from_str("{}").unwrap(),
            request: Request::default(),
            request_set: false,
            local_ip_address: addr.to_string()
        }
    }

    /// Read the OpenAPI file containing this server's endpoints
    /// (Set self.routing_file_read = false for a re-read)
    fn read_api(&mut self){

        // only read it it is not already read
        if !self.routing_file_read {

            // Open the file in read-only mode 
            let open_api_file = match File::open(&self.routing_file_path){
                // @todo: consider process.exit(0)
                Err( _e ) => panic!("Cannot find file with API configuration"),
                Ok ( f ) => f
            };

            info!("Reading routing table (again?) ...");
            self.routing_json = match serde_json::from_reader( BufReader::new( open_api_file )){
                // @todo: consider process.exit(0)
                Err( _ ) => panic!("Cannot parse file with API configuration"),
                Ok( api ) => api
            };

            self.routing_file_read = true;
        }
    }

    /// If POST is used but SELECT syntax needed for db query
    /// (e.g. in login, where the credentials are sent 
    /// through payload in a post request, but a 
    /// stored prodecure called using `SELECT`),
    /// the API.json has `x-query-syntax-of-method: "GET"`.
    ///
    /// accessible through API.request.method_reroute.
    fn check_rerouting( &mut self ){
        if self.request.method == RequestMethod::POST {
            if self.routing_json[ API::API_PATHS ]
                [ &self.request.url ]
                [ Request::get_method_as_str(self.request.method) ]
                [ "x-query-syntax-of-method" ].as_str().unwrap_or("") == "GET" {
                info!("--> POST request re-routed to GET syntax");
                self.request.method_reroute = RequestMethod::POSTasGET;
            }
        }
    }

    // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
    // Important public methods
    ///
    /// Initialise a request for this API
    pub fn set_request( &mut self, request: &Request ){
        self.reset_request();
        self.request = request.clone();
        if request.is_reload_config{
            info!("Request to reload the openAPI endpoint configuration.");
            self.routing_file_read = false;
        }
        self.read_api(); // usually does nothing
        self.check_rerouting( );
        self.check_auth_need( );
        self.request_set = true;
    }

    /// Check request parameters against API and return
    /// a collection of CheckedParameters
    ///
    /// Checking means:
    ///
    /// (1) all required parameters are present
    ///     and of the expected type;
    ///
    /// (2) Parameters that are *not* required are checked for the  
    ///     expected type.
    ///
    /// (3) Unexpected parameters are ignored.
    ///
    pub fn get_checked_query_params( &mut self ) -> &Vec<CheckedParam>{

        // ----------------------------------------------------------------------
        // Interface: Make sure a request is present 
        if !self.request_set { 
            error!("There is no request set to check against this API"); 
            self.checked_query_parameters = vec![];
            // @TODO: consider panic? process.exit(1)?
            return &self.checked_query_parameters;
        };

        // Read query params if they aren't read yet
        if !self.checked_query_params_read { self.check_query_parameters( ); }

        &self.checked_query_parameters
    }
    
    /// Get a list of payload parameters in this POST or PATCH
    /// request that conform to the API.
    pub fn get_checked_post_params( &mut self ) -> &Vec<CheckedParam>{

        // ----------------------------------------------------------------------
        // Make sure a request is present
        if !self.request_set { 
            error!("There is no request set to check against this API"); 
            self.checked_query_parameters = vec![];
            // @TODO: consider panic? process.exit(1)?
            return &self.checked_post_parameters;
        };

        if !self.checked_post_params_read {
            self.check_post_parameters( );
        }

        &self.checked_post_parameters
    }

    /// Where does the request differ from the specification of the API?
    /// Empty if the request accords to the API
    /// @TODO: needs to be stored in object var?
    pub fn get_request_deviation( &mut self ) -> String{

        // ----------------------------------------------------------------------
        // Interface: Make sure a request is present 
        if !self.request_set { 
            error!("There is no request set to check against this API"); 
            self.checked_query_parameters = vec![];
            // @TODO: consider panic? process.exit(1)?
            return S_EMPTY;
        };

        // Check if an authentication is needed, and if so, 
        // if one is set. Contains no validation of token,
        // just a check if one was handed over.
        // @TODO What does this code really do?
        if self.request.api_needs_auth == Authentication::NEEDED {
            if !self.request.has_valid_auth() {return 
                String::from("API requires valid authentication for this request, but none was found");}

            let auth_claim_items = self.get_auth_claim_items_from_api( );
            let mut pg_set = "".to_string();
            for i in auth_claim_items{
                info!("Items: {:?}", i);
                match i.checkval{
                    Some (val) => {
                        info!("...need to check if >{}< is >{}<", i.name, val);
                        let b_ok = match &self.request.auth_claim{
                            Some (e) => e.get( &i.name ).expect("").as_str().expect("") == val,
                            None => false
                        };
                        info!(" ... result: {}", b_ok);
                        if !b_ok {return String::from("Invalid authentication, check token or API");}
                    }
                    None => {}
                };
                match i.pg_set_as{
                    Some( val ) => {
                        let pg_val_to_set = match &self.request.auth_claim{
                            Some (e) => {
                                info!("WWWA? {:?} <- {}", e, &i.name);
                                // Problem: https://docs.serde.rs/serde_json/value/enum.Value.html
                                // is_number tut nicht mit as_str
                                //let x: String = e.get( &i.name ).expect("").into();
                                let x: String = e.get( &i.name ).expect("").to_string();
                                x
                            },
                            None => "".to_string()
                        };
                        if pg_val_to_set != ""{
                            pg_set = format!("{}; SET LOCAL {}.{}='{}';", pg_set, self.pg_setvar_prefix, val, pg_val_to_set);
                            info!("pushing: SET {}.{}'='{}';", self.pg_setvar_prefix, val, pg_val_to_set);
                        }
                    }
                    None => {}
                };
            };

            self.pg_set = pg_set;

            // (1) if there are Checkvals, check them and throw Exceptions on violation
            // (2) the pg_set_as need to go to db in order to set variables on the client.
                //"x-claim-custom": [
                //  {"name": "role", "checkval": "sf_editor"},
                //  {"name": "dozent_id", "pg_set_as": "pg_api_muscle.editor_id"}
                //],
        }

        // @shj 2021-7-25: does this route exist?
        // [Newly needs checking since we're allowing empty parameter lists.]
        if self.routing_json[ API::API_PATHS ]
            [ &self.request.url ]
            [ Request::get_method_as_str(self.request.method) ].is_null() {return "No route for this request.".to_string();}

        // Check params by calling the .get_checked_* methods,
        // hand back problem report
        match self.request.method{
            RequestMethod::GET => {
                self.get_checked_query_params();
                self.problems_query_parameters.to_owned()
            },
            RequestMethod::DELETE => {
                self.get_checked_query_params();
                self.problems_query_parameters.to_owned()
            },
            RequestMethod::PATCH => {
                self.get_checked_query_params();
                &self.get_checked_post_params();
                if self.problems_query_parameters == "No such route" {
                     "No such route".to_string()  // otherwise "No such route" is returned twice
                }else{
                    let mut tmp = self.problems_query_parameters.to_owned();
                    tmp.push_str( &self.problems_post_parameters.to_owned());    // error msges
                    tmp
                }
            },
            RequestMethod::POST => {
                &self.get_checked_post_params();
                self.problems_post_parameters.to_owned()
            }
            _ => { "This request method is not implemented; please use PATCH, POST, GET, or DELETE".to_string() }
        }
    }
    
    /// Get the values of all checked query parameters
    /// as a vector (e.g. for use in a stored procedure)
    pub fn get_checked_query_param_vals( &mut self ) -> Vec<&ParamVal>{
        self.check_query_parameters();
        API::get_param_vals( &self.checked_query_parameters )
    }

    /// Get the values of all checked post parameters
    /// as a vector (e.g. for use in a stored procedure)
    pub fn get_checked_post_param_vals( &mut self ) -> Vec<&ParamVal>{
        self.check_query_parameters();
        API::get_param_vals( &self.checked_post_parameters )
    }

    /// Get the values of all checked post *and* query parameters
    /// combined as a vector (e.g. for use in a stored procedure like 
    /// `update set x=y where a=b`)
    pub fn get_checked_combined_param_vals( &mut self ) -> Vec<&ParamVal>{
        self.check_query_parameters();
        self.check_post_parameters();
        let mut checked_post_values = API::get_param_vals( &self.checked_post_parameters );
        let mut checked_get_values = API::get_param_vals( &self.checked_query_parameters );
        checked_post_values.append( &mut checked_get_values );
        checked_post_values
    }

    /// Name of token to set in database
    /// The name is configured in .env, e.g. token_name=pg_request_token.
    /// This leads to a SET pg_request_token = <Request-TOKEN>
    /// in requests to the db that need authentication.
    pub fn get_pg_token_name( &mut self ) -> &String{
        &self.token_name
    }

    /// Checks if all payload parameters that the API requires for
    /// the request are present and of the expected type.
    fn check_post_parameters( &mut self ){

        debug!("Checking post parameters: looking for {} in {}", self.request.method, self.request.url );

        // Get obligatory parameters for this route. If we find some, ...
        let tmp: Vec<UnCheckedParam> = match self.get_parameters_from_api( API::PARAM_TYPE_PAYLOAD ){
        
            // ...: collect and return error 
            // messages and name/value pairs 
            Some( parms ) => parms.into_iter().map( 
               |par| { API::collect_payload_typecast_problems( &par.name, &par.required,
                            self.request.get_payload_param( &par.name ),
                            &par.schema.r#type)
                    }
                    ).collect(),

            // ... *no* parameters:
            None => vec![UnCheckedParam{
                problem: "No such route".to_string(), 
                name: S_EMPTY, 
                value: ParamVal::Text(S_EMPTY)}]
        };

        // separate problematic from conforming parameters
        self.split_problems_post_parms( &tmp );
        self.checked_post_params_read = true;
    }

    /// Checks if all query parameters that the API requires
    /// for the request are present and of the expected type
    fn check_query_parameters( &mut self ){

        if !self.checked_query_params_read {
            debug!("Looking for {} in {}", self.request.method, self.request.url );

            // Get API requirements for this request and
            // produce a vector of "UnCheckedParam" with 
            // the successfull and problematic aspects
            // of this request
            let tmp = match self.get_parameters_from_api( API::PARAM_TYPE_QUERY ){

                Some( parms ) => parms.into_iter().map( 
                    |par| { API::check_parameter( &par.name, &par.required,
                        &self.request.get_query_param( &par.name ),
                        &par.schema.r#type)
                    }
                ).collect(),

                // ... *no* parameters:
                None => {info!("... no parameters required for this request `{}` method {}", self.request.url, self.request.method); 
                    vec![]
                }
            };

            // separate problematic from conforming parameters
            self.split_problems_query_parms( &tmp );
            self.checked_query_params_read = true;
        }
    }

    // Utility for prepared statement that needs a vector of 
    // just the values of checked parameters
    fn get_param_vals( checked_parameters: &Vec<CheckedParam> ) -> Vec<&ParamVal>{
        checked_parameters.into_iter().map( |y| &y.value ).collect()
    }

    // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
    // Get properties from configuration API file
    
    /// Retrieve the `operationId` (name of the view) of this
    /// request from API.
    pub fn get_operations_id( &mut self ) -> String{
       
        // ----------------------------------------------------------------------
        // Interface: Make sure a request is present 
        if !self.request_set { 
            error!("There is no request set to check against this API"); 
            self.checked_query_parameters = vec![];
            return S_EMPTY;
        };

        match self.routing_json[ API::API_PATHS ]
            [ &self.request.url ]
            [ Request::get_method_as_str(self.request.method) ]
            [ API::API_QUERY ].as_str() {

            Some( path ) => path.to_string(),
            _ => S_EMPTY,
        }
    }

    // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
    // split problems from CheckedParams

    ///
    /// separates query params *with* problems from those without.
    fn split_problems_query_parms(&mut self, query_params: &Vec<UnCheckedParam>){
        let splitter = API::split_problems( query_params );
        self.checked_query_parameters = splitter.0;
        self.problems_query_parameters = splitter.1;
    }

    ///
    /// separates payload params *with* problems from those without.
    fn split_problems_post_parms(&mut self, post_params: &Vec<UnCheckedParam>){
        let splitter = API::split_problems( post_params );
        self.checked_post_parameters = splitter.0;
        self.problems_post_parameters = splitter.1;
    }

    ///
    /// separate UnCheckedParameters into checked parameters and a String containing
    /// problem information.
    fn split_problems( params: &Vec<UnCheckedParam> ) -> ( Vec<CheckedParam>, String ){
        let mut successfully_checked_params:Vec<CheckedParam> = vec![];
        let mut s_problems = "".to_string();

        for unchecked_param in params{
            if API::is_no_problem( &unchecked_param.problem ) {
                if API::is_not_superfluous( &unchecked_param.problem ) 
                    {successfully_checked_params.push( CheckedParam{ name: unchecked_param.name.to_owned(), value: unchecked_param.value.to_owned() } );}
            }else { s_problems.push_str( &unchecked_param.problem );}
        };
        ( successfully_checked_params, s_problems )
    }

    /// The `problem` property of this UnCheckedParam does 
    /// not present a (real) problem:
    ///
    /// it is either empty or a marker that a non-obligatory 
    /// parameter was not handed over.
    ///
    /// (Needed for readability of .split_problems_query_parms)
    fn is_no_problem( par: &String ) -> bool{
        par == "" || par == API::SUPERFLUOUS_PARAMETER 
    }

    /// The `problem` property of this UnCheckedParam does 
    /// *NOT* say that a superfluous parameter was missing.
    ///
    /// (Needed for readability of .split_problems_query_parms)
    fn is_not_superfluous( par: &String ) -> bool{
        par != API::SUPERFLUOUS_PARAMETER
    }

    // @TODO see also ~220: document authentication stuff
    fn get_auth_claim_items_from_api( &mut self ) -> Vec<ClaimItem>{
        let s_method = Request::get_method_as_str( self.request.method );
        let s_path = &self.request.url;
        let result: Vec<ClaimItem> = match serde_json::from_value( self.routing_json[ API::API_PATHS ]
            [ s_path ]
            [ s_method ]
            [ "x-claim-custom" ].clone() ){

            Ok( x ) => x, 
            Err( e ) => {
                error!("Wild claim: Api definition for endpoint `{}`, method `{}`, is no valid JSON: `{}`", 
                    s_path, s_method, e);
                vec![]
            }
        };
        result
    }

    /// Depending on the parameter type (payload or query),
    /// the expected parameters are retrieved from the api
    ///
    /// panics if api no valid JSON.
    /// @TODO method too long.
    /// @TODO IMPORTANT: disambiguate if there IS NO ROUTE from if IT HAS NO PARAMETERS. (Still valid?)
    fn get_parameters_from_api( &mut self, param_type: u8 ) -> Option<Vec<APIParam>> {

        let s_method = Request::get_method_as_str( self.request.method );
        let s_path = &self.request.url;
        let mut res: Vec<APIParam> = vec![];

        if param_type == API::PARAM_TYPE_PAYLOAD {

            // get parameter definition from $ref in openAPI schema
            let s_pointer = match self.routing_json[ API::API_PATHS ]
                [ s_path ]
                [ s_method ]
                [ "requestBody" ]
                [ "content" ]
                [ "application/json" ]
                [ "schema" ]
                [ "$ref" ].as_str(){
                    Some (a) => &a[1..],
                    _ => ""
                };
            
            if s_pointer == "" { return None };

            // The properties of this object. I guess because 
            // of the iteration below, they must be flat.
            let s_props = self.routing_json.pointer( &(s_pointer.to_owned() + 
                    &"/properties".to_owned()) ).unwrap_or(&Value::Null);

            // Wenn der Pointer keine Ergebnisse liefert, gilt die API als nicht fertig
            // konfiguriert. Es wird "No such route" an den Server geliefert und der Fehler
            // geloggt.
            if s_props == &Value::Null { 
                error!("API is missing a components description of `{}` -> no route.", s_pointer);
                return None; 
            }
            
            // The properties that are *required* are listed in an extra 
            // array (openAPI spec https://swagger.io/docs/specification/describing-request-body/)
            let s_required_sub: &Value = self.routing_json.pointer( &(s_pointer.to_owned() + 
                    &"/required".to_owned()) ).unwrap_or( &Value::Null );

            // If no parameters of the object are marked as required,
            // there's nothing to check
            // @PONDER: should this log an error? Or is this merely an INFO?
            // @TODO: should *not* required params not at least be checked for their type?
            if s_required_sub == &Value::Null { 
                error!("API is missing required components list of `{}` -> no route.", s_pointer);
                return None; 
            }

            let s_required: &Vec<Value> = match s_required_sub.as_array(){
                Some (x) => x,
                None => { info!("API with empty required components list of `{}` -> no route.", s_pointer);
                    return None;
                }
            };

            // required is set depending whether s_required contains
            // the name of this parameter or not. #Needs testing
            for (key, val) in s_props.as_object().unwrap() {
                res.push( APIParam{
                    name: key.to_string(), 
                    description: "".to_string(), 
                    r#in: "".to_string(), 
                    required: s_required.contains( &Value::String(key.to_string()) ), 
                    schema: Schema{ r#type: val[ "type" ].as_str().unwrap().to_string(), format: "".to_string() }} );
            };
            return Some (res);
        }

        if param_type == API::PARAM_TYPE_QUERY {
            let s_method_path = match s_method{
                "get" => "get",
                _ => "patch"
            };

            let result2: Vec<APIParam> = match serde_json::from_value( self.routing_json[ API::API_PATHS ]
                [ s_path ]
                [ s_method_path ]
                [ "parameters" ].clone() ){

                Ok( x ) => x, 
                Err( e ) => {
                    debug!("Wild request: Api definition for endpoint `{}`, method `{}`, is no valid JSON: `{}`", 
                       s_path, s_method_path, e);
                    vec![]
                }
            };

            if result2.len() > 0 {return Some ( result2 );}
            else {return None}
        };
       None
    }

    /// This method is called while iterating through the parameter-list
    /// of the API! *Not* iterating through the parameters that are 
    /// actually handed over. (There is no reason to iterate this list, 
    /// really).
    ///
    /// Static method that `checks` a parameter, where a check is:
    ///
    /// (1) if there is a parameter value, it must conform to the type `s_param_value`
    ///
    /// (2) if there is *no* parameter value, then  
    ///
    ///     (a) if the parameter is required, the check fails,
    ///     (b) if the parameter is *not* required, the check 'fails'
    ///         with the problem set to SUPERFLUOUS_PARAMETER.
    ///
    /// @TODO: call hierarchy of this function? Do I need to hand s_param_type as string at this point?
    fn check_parameter( s_param_name: &str, 
        b_param_required: &bool, 
        s_param_value: &Option<&str>, 
        s_param_type: &str ) -> UnCheckedParam{

        // Do we have a value or not? ...
        match s_param_value{

            // ... if there *is* a value, check its type: ...
            Some ( value ) => match API::get_type_as_configured( &value.to_string(), s_param_type ){

                // if the type conforms to the api, hand back no problms
                Ok( val ) => UnCheckedParam{ 
                    problem: S_EMPTY, 
                    name: s_param_name.to_string(), 
                    value: val},

                    // ... if type is wrong, return (ERR, EMPTY, value)
                _ => UnCheckedParam{ 
                    problem: format!("parameter \"{}\" is expected to be of \
                             type \"{}\", but its value \"{}\" is \
                             not.", s_param_name, s_param_type, &value.to_string()), 
                        name: S_EMPTY, 
                        value: ParamVal::Text(S_EMPTY)
                    }
            },

            // ... if there *is no* value handed over, return (ERR, EMPTY, EMPTY)
            None => {
                if *b_param_required {
                    return UnCheckedParam{ 
                        problem: format!("parameter \"{}\" is obligatory according to api \
                                            but missing from the request", s_param_name), 
                            name: S_EMPTY, 
                            value: ParamVal::Text(S_EMPTY)
                    };
                }else{
                    return UnCheckedParam{
                        problem: API::SUPERFLUOUS_PARAMETER.to_string(), 
                        name: S_EMPTY, 
                        value: ParamVal::Text(S_EMPTY)};
                }

            }
        }
    }

    // @TODO type of Date, type of JSON?
    fn get_type_as_configured( s_param_value: &str, s_param_type: &str ) -> Result<ParamVal, String>{
        match s_param_type {
            API::TYPE_STRING => Ok( ParamVal::Text( s_param_value.to_string() )),
            API::TYPE_INTEGER => match s_param_value.parse::<i32>().is_ok(){
                true => Ok( ParamVal::Int( s_param_value.parse::<i32>().unwrap()) ),
                false => Err(format!("Not an integer value: `{}`", s_param_value))
            }
            API::TYPE_BIGINT => match s_param_value.parse::<i64>().is_ok(){
                true => Ok( ParamVal::BigInt( s_param_value.parse::<i64>().unwrap()) ),
                false => Err(format!("Not a bigint value: `{}`", s_param_value))
            }
            API::TYPE_BOOLEAN => match s_param_value.parse::<bool>().is_ok(){
                true => Ok( ParamVal::Boolean( s_param_value.parse::<bool>().unwrap()) ),
                false => Err(format!("Not a boolean value: `{}`", s_param_value))
            }
            API::TYPE_NUMBER => match s_param_value.parse::<f64>().is_ok(){
                true => Ok( ParamVal::Float( s_param_value.parse::<f64>().unwrap()) ),
                false => Err(format!("Not a float number: `{}`", s_param_value))
            }
            _ => Err(format!("Not a known type: `{}`", s_param_type))
        }
    }

    fn typetest_i64( val: &Value, s_param_name: &str ) -> UnCheckedParam{
        match val.is_i64(){
            true => UnCheckedParam{ problem: S_EMPTY, name: s_param_name.to_string(), 
                value: ParamVal::Int(val.as_i64().unwrap().try_into().unwrap())},

            _ => UnCheckedParam{ problem:format!("parameter \"{}\" is expected to be of \
                type \"integer\", but its value \"{}\" is \
                not.", s_param_name, val), name: S_EMPTY, value: ParamVal::Text(S_EMPTY)}
        }
    }

    fn typetest_bool( val: &Value, s_param_name: &str ) -> UnCheckedParam{
        match val.is_boolean() {
            true => UnCheckedParam{ problem: S_EMPTY, name: s_param_name.to_string(), 
                value: ParamVal::Boolean(val.as_bool().unwrap())},

            _ => UnCheckedParam{
                problem: format!("parameter \"{}\" is expected to be of \
                type \"boolean\", but its value \"{}\" is \
                not.", s_param_name, val), name: S_EMPTY, value: ParamVal::Text(S_EMPTY)}
        }
    }

    fn typetest_number( val: &Value, s_param_name: &str ) -> UnCheckedParam{
        match val.is_f64() {
            true => UnCheckedParam{ problem: S_EMPTY, name: s_param_name.to_string(), 
                value: ParamVal::Float(val.as_f64().unwrap())},

            _ => UnCheckedParam{
                problem: format!("parameter \"{}\" is expected to be of \
                            type \"number\", but its value \"{}\" is \
                            not.", s_param_name, val), name: S_EMPTY, value: ParamVal::Text(S_EMPTY)}
        }
    }

    // new 2021-6
//    fn typetest_json( val: &Value, s_param_name: &str ) -> UnCheckedParam{
//        match val.is_() {
//            true => UnCheckedParam{ problem: S_EMPTY, name: s_param_name.to_string(), 
//                value: ParamVal::Float(val.as_f64().unwrap())},
//
//            _ => UnCheckedParam{
//                problem: format!("parameter \"{}\" is expected to be of \
//                            type \"number\", but its value \"{}\" is \
//                            not.", s_param_name, val), name: S_EMPTY, value: ParamVal::Text(S_EMPTY)}
//        }
//    }

    // @TODO really? I don't get this. Why not s = val.as_string()?
    fn typetest_string( val: &Value, s_param_name: &str ) -> UnCheckedParam{
        let s = match val.as_str(){
            Some( e ) => e.to_string(),
            _ => val.to_string()
        };
        UnCheckedParam{ problem: S_EMPTY, name: s_param_name.to_string(), 
                        value: ParamVal::Text( s )}
    }

    // @TODO typechecking seems full of redundancy in a go-through 
    // and probably needs patient analysis
    fn report_missing_parameter( s_param_name: &str, s_param_type: &str, b_required: bool ) -> UnCheckedParam{
        if b_required {
            UnCheckedParam{ problem: format!("parameter \"{}\" is expected to be of \
                            type \"{}\", but it seems missing", s_param_name, s_param_type), 
                            name: S_EMPTY, value: ParamVal::Text(S_EMPTY)}
        }else{
            UnCheckedParam{ problem: API::SUPERFLUOUS_PARAMETER.to_string(), 
                name: S_EMPTY, 
                value: ParamVal::Text(S_EMPTY)}
        }
    }

    // Unwraps the value, reports a "missing parameter" if this fails,
    // and type-checks the value otherwise.
    fn report_missing_or_typecheck( s_param_name: &str, b_param_required: &bool, 
        s_param_value: Option<&Value>, s_param_type: &str, 
        typetest: &dyn Fn( &Value, &str ) -> UnCheckedParam) -> UnCheckedParam{

        match s_param_value{
            Some( p_val ) => typetest( p_val, s_param_name ),
            _ => {
                info!("Missing `{:?}`, `{}` ??", s_param_value, s_param_name);
                API::report_missing_parameter( s_param_name, s_param_type, *b_param_required )
            }
        }
    }

    /// Problems of type-safety in the request (vs. the expected type) 
    /// are stored in an `UnCheckedParam` which is later split into 
    /// serious problems and `CheckedParams.`
    /// 
    /// The result:
    /// problem: [ description | empty ]
    /// name: [ name ]
    /// value: [ value ]
    fn collect_payload_typecast_problems( s_param_name: &str, b_param_required: &bool, 
        s_param_value: Option<&Value>, s_param_type: &str ) -> UnCheckedParam{

        match s_param_type{

            // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
            // integer
            "integer" => API::report_missing_or_typecheck( s_param_name, b_param_required, 
                                s_param_value, s_param_type, &API::typetest_i64 ),

            // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
            // bigint
            "bigint" => API::report_missing_or_typecheck( s_param_name, b_param_required, 
                                s_param_value, s_param_type, &API::typetest_i64 ),

            // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
            // string
            "string" => API::report_missing_or_typecheck( s_param_name, b_param_required, 
                                s_param_value, s_param_type, &API::typetest_string ),

            // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
            // boolean
            "boolean" => API::report_missing_or_typecheck( s_param_name, b_param_required, 
                                s_param_value, s_param_type, &API::typetest_bool ),

            // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
            // float
            "number" =>  API::report_missing_or_typecheck( s_param_name, b_param_required, 
                                s_param_value, s_param_type, &API::typetest_number ),

            _ => { UnCheckedParam{
                problem: format!("parameter \"{}\" is expected to be of \
                         type \"{}\", but this type is not implemented in this library.", 
                         s_param_name, s_param_type), 
                name:    S_EMPTY, 
                value:   ParamVal::Text(S_EMPTY)}
            }
        }
    }

    /// ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
    /// Less interesting code

    // Re-use API with every new request:
    fn reset_request( &mut self ){
        self.checked_query_parameters = vec![];
        self.problems_query_parameters = S_EMPTY;
        self.checked_query_params_read = false;
        self.checked_post_parameters = vec![];
        self.problems_post_parameters = S_EMPTY;
        self.checked_post_params_read = false;
        self.request_set = false;
    }

    /// Sets the "api_needs_auth" flag
    /// in the request, if the api for
    /// this request contains 
    /// "x-auth-method":"forward_jwt_bearer",
    fn check_auth_need( &mut self ){
        if self.routing_json[ API::API_PATHS ]
            [ &self.request.url ]
            [ Request::get_method_as_str(self.request.method) ]
            [ "x-auth-method" ] == "forward_jwt_bearer" {
                info!("...needs JWT authentication");
                self.request.api_needs_auth = Authentication::NEEDED;
        }else{
                self.request.api_needs_auth = Authentication::NOTNEEDED;

        }
    }


}
