use deadpool_postgres::{Pool };
use crate::API;
use crate::ParamVal;
use crate::RequestMethod;
use crate::Authentication;
use crate::CheckedParam;
use tokio_postgres::{Client};
use tokio_postgres::types::ToSql;
use log::{error, info};

const EMPTY_RESULT: &str = "{}"; // empty string is no JSON

/// 
/// Get a JSON result from the database.
///
/// tokio_postres-methods methods used:
///
/// get_first_row 
/// --------------
/// Query database and retrieve the first 
/// row. (Queries 
/// ask for json aggregates of data, so 
/// usually the first row contains all 
/// information in an array).
///
/// query_execute
/// --------------
/// Executes an SQL command (update, delete,
/// insert) with parameters and delivers
/// the count of rows affected.
///
/// exe_count_limit
/// --------------
/// Returns the result of an update or 
/// insert, can limit rows, which is 
/// currently under construction, though
///
pub async fn get_db_response( pool: &Pool, api: &mut API ) -> Result<String, String>{
   let http_method = api.request.method;         
   let needs_auth = api.request.api_needs_auth == Authentication::NEEDED; // JWT Token needed?

   let mut client = match pool.get().await{
       Ok (cl) => cl,
       Err( e ) => {return Err(format!("No db client available: {:?}", e)); }
   };

   // #HACK Timezone 
   adjust_timezone( &mut client, "Europe/Berlin" ).await;

   if needs_auth{
       set_auth( &mut client, &api.get_pg_token_name().clone(), &api.request.get_auth(), &api.pg_set ).await;
   }

   // -------------------------------------------------------------------------------- 
   // Matching HTTP methods:
   // GET => Select
   // POST => Insert into or Select
   // DELETE => Delete
   // PATCH => Update ... Where
   match http_method{
       // ---------------------------------------- 
       // GET
       RequestMethod::GET => {
           let sql = get_db_get_sql( api );
           query_db( &mut client, needs_auth, &api.get_checked_query_param_vals(), &sql, http_method).await
       },

       // ---------------------------------------- 
       // DELETE
       RequestMethod::DELETE => {
           let sql = get_db_delete_sql( api );
           query_db( &mut client, needs_auth, &api.get_checked_query_param_vals(), &sql, http_method).await
       },

       // ---------------------------------------- 
       // POST
       RequestMethod::POST => {
           let sql = get_db_post_sql( api );
           query_db( &mut client, needs_auth, &api.get_checked_post_param_vals(), &sql, http_method).await
      },

      // ---------------------------------------- 
      // PATCH
       RequestMethod::PATCH => {
           let sql = get_db_patch_sql( api );
           query_db( &mut client, needs_auth, &api.get_checked_combined_param_vals(), &sql, http_method).await
       },

       _ => Err( "Methode nicht implementiert".to_string() )
   }
}

/// IN:
/// client: db connection client,
/// unset_auth_after_query: clean token and UNSET pg variables after execution?
/// query_parameters: values corresponding to $1, $2, ..., $n
/// s_sql: SQL command with $1, $2, ..., $n
/// method: HTTP-Request method of this request.
///
/// OUT:
/// returns: JSON response from DB, or Error (String).
///
/// LOGS:
/// error: "EB failure" + error information
async fn query_db( 
    client: &mut Client, 
    clean_auth_after_query: bool,
    query_parameters: &Vec<&ParamVal>,
    sql: &str,
    method: RequestMethod) -> Result<String, String>{

    // query method is query_execute_string (for DELETE) or get_first_row (for 
    // INSERT, UPDATE, SELECT).
    if method != RequestMethod::DELETE {
        match get_first_row( client, &sql, Some( &query_parameters ) ).await{
            Ok( r ) => {if clean_auth_after_query {unset_auth( client ).await;} Ok(r)},
            Err( e ) => {if clean_auth_after_query {unset_auth( client ).await;}
                error!("DB failure: `{}`", e);
                Err( format!("Database could not complete the request: `{}`", e))
            }
        }
    }else{
        match query_execute_string( client, &sql, Some( &query_parameters ) ).await{
            Ok( r ) => {if clean_auth_after_query {unset_auth( client ).await;} Ok(r)},
            Err( e ) => {if clean_auth_after_query {unset_auth( client ).await;}
                error!("DB failure: `{}`", e);
                Err( format!("Database could not complete the request: `{}`", e))
            }
        }
    }
}

// Build SQL String for a patch request -> update ...
fn get_db_patch_sql( api: &mut API ) -> String{
        let query = &api.get_operations_id();    // The query
        match api.get_checked_query_params().len(){
            0 => format!("update {} set {} returning row_to_json({}.*)::text;", query, 
                    get_parameter_assignment_csv( &api.get_checked_post_params( ) ), query),
            _ => format!("update {} set {} where ({}) returning row_to_json({}.*)::text;", 
                    query, 
                    get_parameter_assignment_csv( &api.get_checked_post_params( ) ), 
                    get_parameter_where_criteria( api ), 
                    query)
        }
}

// Build SQL String for a post request:
// (1) either insert into ... or
// (2) select X from a stored proc. 
// The indicator for (2) is: "x-query-syntax-of-method":"GET"
// fn get_db_post_sql( api: &mut API ) -> String{
fn get_db_post_sql( api: &mut API ) -> String{

       let query = &api.get_operations_id(  );    // The query

       // "Reroute" is a special functionality for POST 
       // requests that need GET-treatment:
       // OpenAPI has the attribute "x-query-syntax-of-method": "GET",
       // e.g. if a stored proc is called, we 
       // want 'select' rather than 'insert into'.
       match api.request.method_reroute {
           RequestMethod::POSTasGET => {
               format!("select json_agg(t)::text from (select * from {} ({})) t;", 
                   query, 
                   get_sql_named_notation_from_params( &api.get_checked_post_params( ) ))
           },

           // Default for POST is 'insert into,' though.
           _ =>{ format!("insert into {} ({}) values ({}) returning row_to_json({}.*)::text;", 
               query, 
               get_parameter_names_csv( &api.get_checked_post_params() ), 
               get_parameter_placeholder_csv( &api.get_checked_post_params(), None ), 
               query)
           }
       }
}

/// Build SQL String for a get request -> select * from
fn get_db_get_sql( api: &mut API ) -> String{

    let query = &api.get_operations_id( );    // The query

    // Case 0 means: there are no parameters.
    match api.get_checked_query_params().len(){
        0 => format!("select json_agg(t)::text from (select * from {}) t;", query),
        _ => format!("select json_agg(t)::text from (select * from {} where {}) t;", 
            query, get_parameter_where_criteria( api ))
    }
}

// Build SQL String for a delete request -> delete * from
fn get_db_delete_sql( api: &mut API ) -> String{

       let query = &api.get_operations_id( );    // The query

        match api.get_checked_query_params().len(){
            0 => format!("delete from {};", query),
            _ => format!("delete from {} where {};", query, get_parameter_where_criteria( api ))
        }
}

// ==================================================================================
// Helper SQL
//
/// Helper SQL for `insert into...` statements
///
/// Extracts comma separated list of names from the checked parameters.
///
/// let checked_parameter = vec![CheckedParam{ name: "y".to_string(), value: ParamVal::Text("z".to_string())}, 
///                 CheckedParam{ name: "b".to_string(), value: ParamVal::Text("c".to_string())}];
///
/// assert_eq!( get_parameter_names_csv( &checked_parameter ), "\"y\",\"b\"");
///
fn get_parameter_names_csv( request: &Vec<CheckedParam>  ) -> String{
    request.into_iter().map( |y| { format!(",\"{}\"", &y.name)} ).collect::<String>().chars().skip(1).collect()
}

/// Helper SQL for stored procedure calls with named parameter syntax
///
/// Extracts comma separated list of named-notation from the checked parameters.
///
/// let checked_parameters= vec![CheckedParam{ name: "y".to_string(), value: ParamVal::Text("z".to_string())}, 
///                 CheckedParam{ name: "b".to_string(), value: ParamVal::Text("c".to_string())}];
///
/// assert_eq!( get_sql_named_notation_from_params( &checked_parameters ), "\"y\"=>$1,\"b\"=>$2");
fn get_sql_named_notation_from_params( request: &Vec<CheckedParam>  ) -> String{
    let mut ii = 0;
    request.into_iter().map( |y| { 
        ii = ii + 1; 
        format!(",\"{}\"=>${}", &y.name, ii)} ).collect::<String>().chars().skip(1).collect()
}

/// Helper SQL for `update ... where` statements
///
/// Extracts comma separated list of SQL assignments.
///
/// let tt = vec![CheckedParam{ name: "y".to_string(), value: ParamVal::Text("z".to_string())}, 
///             CheckedParam{ name: "b".to_string(), value: ParamVal::Text("c".to_string())}];
///
/// assert_eq!( get_parameter_assignment_csv( &tt ), "\"y\"=$1,\"b\"=$2");
fn get_parameter_assignment_csv( request: &Vec<CheckedParam>  ) -> String{
    let mut ii = 0;
    request.into_iter().map( |y| { 
        ii = ii + 1; 
        format!(",\"{}\"=${}", &y.name, ii)} ).collect::<String>().chars().skip(1).collect()
}


/// Helper SQL for `select ... from where ...` and `update ... set ...` statements
///
/// Extracts `and`-separated list of SQL assignments; the parameter numbers ($1, $2, ...)
/// start with 1 or -- if the current request is a PATCH -- with n+1, where 
/// n is the number of posted parameters.
///
/// If the parameters are {"name": "id", value: 1}, {"name": "salary", value: 2000 } in a GET
/// get_parameter_where_criteria will return "id=$1 and salary=$2".
///
/// If the parameters are {"name": "id", value: 1}, {"name": "salary", value: 2000 } in a PATCH
/// that has a payload of {"company":200, "year":2021}, the it will return
/// id=$3 and salary=$4 (as in: update X set company=$1 and year=$2 where id=$3 and "salary"=$4.
fn get_parameter_where_criteria( api: &mut API ) -> String{

    let mut ii = match api.request.method{
        RequestMethod::PATCH => api.get_checked_post_params().len(),
        _ => {
            if api.get_checked_post_params().len() > 0{
                error!("Strange: getting post params {:?}", api.get_checked_post_params());
            }
            0
        }
    }; 

    api.get_checked_query_params().into_iter().map( |y| { 
        ii+=1;
        format!("and \"{}\"=${} ", &y.name, ii)}  ).collect::<String>().chars().skip(4).collect()
}

/// Helper SQL for `insert into ...` statements
///
/// Produces a String $1,$2,$3,...,$n (n=params.count), or a 
/// String $k,$(k+1),$(k+2),...$(k+n) (k=start_arg, n=params.count)
///
/// let tt = vec![CheckedParam{ name: "y".to_string(), value: ParamVal::Text("z".to_string())}, 
///             CheckedParam{ name: "b".to_string(), value: ParamVal::Text("c".to_string())}];
///
/// assert_eq!( get_parameter_placeholder_csv( &tt, None  ), "$1,$2");
/// assert_eq!( get_parameter_placeholder_csv( &tt, Some( 7 )  ), "$8,$9");
fn get_parameter_placeholder_csv( parms: &Vec<CheckedParam>, start_arg: Option<usize> ) -> String{
     let mut ii = start_arg.unwrap_or( 0 );
     parms.into_iter().map( |_| { ii+=1; format!(",${}", ii) } ).collect::<String>().chars().skip(1).collect()
}

// ==================================================================================
// DB Interaction
//
/**
 * @todo: verwendet query_opt (anstatt query_one), da bei PATCH sonst eine leere Antwort Probleme
 * macht. 
 *
 * Problem immer noch: Ein PATCH mit falschen Auth Bearer wird u.U. nur durch eine leere Antwort
 * (aber HTTP Status 200) quittiert. Man kann sich streiten, ob das nicht besser ein Fehler wäre.
 * Andererseits kann das vermutlich die SQL Fkt. leisten?
 *
 * @todo: leere Antwort gibt "{}" zurück -- konfigurierbar, ob JSON Antwort oder Txt?
 **/
async fn get_first_row(client: &mut Client, s_sql: &str, prep_vals_opt: Option<&Vec::<&ParamVal>>) ->Result<String, tokio_postgres::Error>{ 

    let row = match client.query_opt( s_sql, &get_pg_parameter_vector( prep_vals_opt )).await{
       Ok ( row ) => row,
       Err ( e ) => {return Err(e); }
    };


    match row{
        Some( result ) => {
            match result.get(0){
                Some (x) => Ok(x),
                _ => Ok( EMPTY_RESULT.to_string() )
            }
        },
        None => Ok( EMPTY_RESULT.to_string() )
    }
}

/// executes the SQL and returns {"message":"rows affected: <nor>"}, with nor = number of rows
/// affected.
async fn query_execute_string(client: &mut Client, s_sql: &str, prep_vals: Option< &Vec::<&ParamVal>> ) -> Result<String, tokio_postgres::Error>{
    match client.execute( s_sql, &get_pg_parameter_vector(  prep_vals )).await{
        Ok( e ) => Ok( format!("{{\"message\":\"rows affected: {}\"}}", e ) ),
        Err( f ) => Err( f )
    }
}

/**
 * Finish the local transaction esp. to invalidate
 * the config parameter `request.pg_api_muscle.token='TOKEN'` 
 **/
async fn unset_auth( client: &mut Client ){
    match client.batch_execute( &format!("END;")[..]).await{
        Ok( _ ) => (),
        Err ( e ) => error!("Transaction FAIL: {}", e)
    }
}

/**
 * Set local config parameter `request.pg_api_muscle.token='TOKEN'` in tokio_postgres.
 * A transaction in client would have looked better, but tests showed 
 * that there is no automatic rollback if the transaction 
 * is not commited. Maybe try again later (@TODO)
 *
 * token name is now configured in the .env file
 **/
async fn set_auth( client: &mut Client, s_token_name: &str, s_auth: &str, s_pg_set: &str ){
    match client.batch_execute( &format!("BEGIN; SET LOCAL {}='{}';{};", s_token_name, s_auth, s_pg_set )[..] ).await{
        Ok( _ ) => {},
        Err( e ) => { panic!("Error transferring the auth token to the database: `{}`", e); }
    };
}

// @TODO: use this, also in get_first_row
fn get_pg_parameter_vector<'a>( raw_values: Option<&Vec::<&'a ParamVal>> ) -> Vec<&'a(dyn ToSql + Sync)>{
   match raw_values{
       Some (vals) => vals.iter().map(|x| get_to_sql_from_param_val( x )).collect(),
       None => [].to_vec()
   }
}

// ===========================================================================
// B O R I N G   C O D E
// ===========================================================================

fn get_to_sql_from_param_val( par: &ParamVal ) -> &(dyn ToSql + Sync){
    match par{
        ParamVal::Text(e) => e as &(dyn ToSql + Sync),
        ParamVal::Int(e) => e as &(dyn ToSql + Sync),
        ParamVal::BigInt(e) => e as &(dyn ToSql + Sync),
        ParamVal::Float(e) => e as &(dyn ToSql + Sync),
        ParamVal::Boolean(e) => e as &(dyn ToSql + Sync),
        ParamVal::Date(e) => e as &(dyn ToSql + Sync)
    }
}

/**
 *  https://github.com/sfackler/rust-tokio_postgres/issues/147
 **/
pub async fn adjust_timezone( mut client: &mut Client, tz: &str) {
    info!("Db-init, tz: initial timezone: {}", get_first_row( &mut client, "show timezone", None ).await.unwrap());
    info!("Db-init, tz: initzal time: {}", get_first_row( &mut client, "select now()::text;", None).await.unwrap());
    info!("Db-init, tz: set tz to {}: {}", tz, client.execute( &format!("set timezone='{}';", tz)[..], &[] ).await.unwrap());
    info!("Db-init, tz: current timezone: {}", get_first_row( &mut client, "show timezone" ,None).await.unwrap());
    info!("Db-init, tz: current db time: {}", get_first_row( &mut client, "select now()::text;",None ).await.unwrap());

}

#[cfg(test)]
mod test_get_named_notation{
    use super::*;

    #[test]
    fn simple() {
        let tt = vec![CheckedParam{
            name: "y".to_string(), 
            value: ParamVal::Text("z".to_string())}, 

            CheckedParam{
                name: "b".to_string(), 
                value: ParamVal::Text("c".to_string())}];

        assert_eq!( get_sql_named_notation_from_params( &tt ), "\"y\"=>$1,\"b\"=>$2");
    }

    #[test]
    fn utf() {
        let tt = vec![CheckedParam{
            name: "Hänßgen".to_string(), 
            value: ParamVal::Text("Ä".to_string())}, 

            CheckedParam{
                name: "Vröß".to_string(), 
                value: ParamVal::Text("c".to_string())}];
        assert_eq!( get_sql_named_notation_from_params( &tt ), "\"Hänßgen\"=>$1,\"Vröß\"=>$2");
    }

    #[test]
    fn empty() {
        let tt = vec![];
        assert_eq!( get_sql_named_notation_from_params( &tt ), "");
    }
}

#[cfg(test)]
mod test_get_sql_insert{
    use super::*;

    #[test]
    fn simple() {
        let tt = vec![CheckedParam{
            name: "y".to_string(), 
            value: ParamVal::Text("z".to_string())}, 

            CheckedParam{
                name: "b".to_string(), 
                value: ParamVal::Text("c".to_string())}];

        assert_eq!( get_parameter_names_csv( &tt ), "\"y\",\"b\"");
    }

    #[test]
    fn utf() {
        let tt = vec![CheckedParam{
            name: "Hänßgen".to_string(), 
            value: ParamVal::Text("Ä".to_string())}, 

            CheckedParam{
                name: "Vröß".to_string(), 
                value: ParamVal::Text("c".to_string())}];
        assert_eq!( get_parameter_names_csv( &tt ), "\"Hänßgen\",\"Vröß\"");
    }

    #[test]
    fn empty() {
        let tt = vec![];
        assert_eq!( get_parameter_names_csv( &tt ), "");
    }
}

#[cfg(test)]
mod test_get_sql_update_parameters{
    use super::*;

    #[test]
    fn simple() {
        let tt = vec![CheckedParam{
            name: "y".to_string(), 
            value: ParamVal::Text("z".to_string())}, 

            CheckedParam{
                name: "b".to_string(), 
                value: ParamVal::Text("c".to_string())}];

        assert_eq!( get_parameter_assignment_csv( &tt ), "\"y\"=$1,\"b\"=$2");
    }

    #[test]
    fn utf() {
        let tt = vec![CheckedParam{
            name: "Hänßgen".to_string(), 
            value: ParamVal::Text("Ä".to_string())}, 

            CheckedParam{
                name: "Vröß".to_string(), 
                value: ParamVal::Text("c".to_string())}];
        assert_eq!( get_parameter_assignment_csv( &tt ), "\"Hänßgen\"=$1,\"Vröß\"=$2");
    }

    #[test]
    fn empty() {
        let tt = vec![];
        assert_eq!( get_parameter_assignment_csv( &tt ), "");
    }
}

#[cfg(test)]
mod test_get_sql_get_parameter_placeholder_csv{
    use super::*;

    #[test]
    fn simple() {
        let tt = vec![CheckedParam{
            name: "y".to_string(), 
            value: ParamVal::Text("z".to_string())}, 

            CheckedParam{
                name: "b".to_string(), 
                value: ParamVal::Text("c".to_string())}];

        assert_eq!( get_parameter_placeholder_csv( &tt, None  ), "$1,$2");
        assert_eq!( get_parameter_placeholder_csv( &tt, Some( 7 )  ), "$8,$9");
    }

    #[test]
    fn utf() {
        let tt = vec![CheckedParam{
            name: "Hänßgen".to_string(), 
            value: ParamVal::Text("Ä".to_string())}, 

            CheckedParam{
                name: "Vröß".to_string(), 
                value: ParamVal::Text("c".to_string())}];
        assert_eq!( get_parameter_placeholder_csv( &tt, None ), "$1,$2");
        assert_eq!( get_parameter_placeholder_csv( &tt, Some( 7 )  ), "$8,$9");
    }

    #[test]
    fn empty() {
        let tt = vec![];
        assert_eq!( get_parameter_placeholder_csv( &tt, None ), "");
        assert_eq!( get_parameter_placeholder_csv( &tt, Some (800) ), "");
    }
}
