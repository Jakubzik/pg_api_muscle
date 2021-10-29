use crate::CPRelation;
use crate::API;
use crate::S_EMPTY;
use std::convert::TryInto;

use log::info;

//#[json]
use serde_json::Value;

///
/// Parameters serve to check against API (are all defined parameters present
/// and of the expected type?)
///
/// ParametersToCheck (unlucky name) differentiates between payload
/// and query paramteters
///
/// These parameters can be initialized as extended (with =eq.-Syntax)
/// or plain.
///
/// @TODO: Do we need both ParametersToCheck and CheckedParams?
/// @TODO: Implement Date
///
pub enum ParameterType{
    STRING,
    INTEGER,
    BIGINT,
    BOOLEAN,
    NUMBER,
    UNKNOWN
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

// Needed for cloning
impl Default for ParamVal {
    fn default() -> Self { ParamVal::Text( "not initialized".to_string() ) }
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

#[derive(Debug, Default, Clone, PartialEq)]
pub struct ParameterToCheck{
    pub problem: String,
    pub name: String,
    relation: CPRelation,           // If 'extended syntax' is used, =5 must be handed over as =eq.5,
    use_extended_syntax: bool,      // and =lt.6 represents <6 for the database. Possible relations (=, < etc.)
    pub value: ParamVal                 // are represented in CPRelation 
}

#[derive(Debug, Clone)]
pub struct CheckedParam{
    pub name: String,
    pub relation: CPRelation,
    pub value: ParamVal
}

impl CheckedParam {
    pub fn new(name: String, value: ParamVal) -> Self { CheckedParam { name, relation: CPRelation::Equal, value } }
    pub fn new_ext(name: String, value: ParamVal, relation: CPRelation) -> Self { CheckedParam { name, relation, value } }
}

/**
 * ParametersToCheck represent both query parameters (which 
 * come by name and a String value) and payload parameters.
 * 
 * Payload parameters come in a Serde value (rather than a String
 * value)
 */
impl ParameterToCheck{ 
    pub fn new_query_parameter(name: &str, value: &str, expected_type: ParameterType) -> Self{
        let check = ParameterToCheck::get_typecheck_of_query_parameter( value, expected_type ); // .1 ist Problem, .0 ist value
        ParameterToCheck { problem: check.1, name: name.to_string(), relation: CPRelation::Unknown, value: check.0, use_extended_syntax: false } 
    }

    // Query parameter with 'extended syntax,'
    // meaning that = is represented as =eq.,
    // < is represented as =lt. etc.
    pub fn new_query_parameter_ext(name: &str, value: &str, expected_type: ParameterType) -> Self{
        let extension = ParameterToCheck::analyze_extended_val( value );
        if extension.1 == CPRelation::Unknown{ ParameterToCheck::new_err_query_ext_param_with_unknown_relation(name, value)}
        else{
            let check = ParameterToCheck::get_typecheck_of_query_parameter( &extension.0[..], expected_type ); // .1 ist Problem, .0 ist value
            ParameterToCheck { problem: check.1, name: name.to_string(), relation: extension.1, value: check.0, use_extended_syntax: true } 
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
                let check=ParameterToCheck::get_typecheck_of_payload_parameter( value, expected_type );
                ParameterToCheck { problem: check.1, name: name.to_string(), relation: CPRelation::Unknown, value: check.0, use_extended_syntax: false } 
            },
            None => {
                if required{ ParameterToCheck::new_err_missing_parameter(name)}
                else{panic!("Something wrong: there's an unrequired parameter without a value ... ?");}
            }

        }
    }

    // Used if there is a POST/PATCH request but no
    // parameters configured in the API -> there 
    // is no route.
    pub fn new_err_no_route() -> Self{
        ParameterToCheck { problem: "No such route".to_string(), name: S_EMPTY, relation: CPRelation::Unknown, 
            value: ParamVal::Text(S_EMPTY), use_extended_syntax: false } 
    }

    pub fn new_err_missing_parameter( s_name: &str ) -> Self{
        ParameterToCheck { problem: format!("parameter \"{}\" is obligatory according to api, but missing from the request", s_name), 
            name: S_EMPTY, relation: CPRelation::Unknown, value: ParamVal::Text(S_EMPTY), use_extended_syntax: false } 
    }

    pub fn new_err_query_ext_param_with_unknown_relation( s_name: &str, s_value: &str ) -> Self{
        ParameterToCheck { problem: format!("parameter \"{}\" is handed over as \"extended,\" but value \"{}\" does not contain a \
            recognizable relation. (Extended parameter have values such as eq.7 for \"equals 7\")", s_name, s_value), 
            name: S_EMPTY, relation: CPRelation::Unknown, value: ParamVal::Text(S_EMPTY), use_extended_syntax: true } 
    }

    // Parameter is in API, but not marked as required 
    // and not in the request. In short, not a problem.
    pub fn new_err_non_required_parameter_missing() -> Self{
        ParameterToCheck { problem: API::SUPERFLUOUS_PARAMETER.to_string(), name: S_EMPTY, relation: CPRelation::Unknown, 
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