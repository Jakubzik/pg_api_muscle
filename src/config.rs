use tini::Ini;
use std::{collections::HashMap,net::Ipv4Addr};

// ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
// MuscleConfiguration parameters
#[derive (Debug)]
pub struct MuscleConfigContext{
    pub prefix: String,                  // Prefix of the ressources of this context
    pub db: String,                      // Name of Postgres (Pg) db
    pub db_user: String,                 // Name of Pg user
    pub db_pass: String,                 // Password of Pg user
    pub api_conf: String,                // OpenAPI config file containing endpoints
    pub pg_service_prefix: String,       // Path-prefix that indicates that this is a db request
    pub token_name: String,              // Pg token name: @TODO
    pub token_secret: String,            // Pg shared token secret: @TODO
    pub pg_setvar_prefix: String,        // Pg prefix for variables that are set in postgres through the token: @TODO
    pub static_404_default: String,      // Default Err page for "not found" -- none if set to "none"
    pub dynamic_err: String,             // Default Err JSON msg for errors in dynamic requests (or "none", meaning detailed error messages will be returned instead)
    pub index_file: String,              // File to return if a folder is requested (or "none")
    pub use_eq_syntax_on_url_parameters: bool // translate https://url?param=eq.5 to "param=5" (...lt.5 to "param < 5"). @TODO. true not yet implemented (August 24, 21)
}
pub struct MuscleConfigCommon{
    pub port: usize,                     // Server port
    pub addr: String,                    // Server address
    pub client_ip_allow: Ipv4Addr,        //
    pub cert_pass: String,               // Pwd for server certificate (TLS/Https)
    pub cert_file: String,               // Certificate file (TLS/Https)
    pub server_use_https: bool,           // Listen for https requests (true) or http?
    pub contexts: HashMap<String, MuscleConfigContext>,
    pub timezone: String,                // Timezone to set Pg to
    pub server_read_timeout_ms: u64,     // Tweak @TODO
    pub server_read_chunksize: usize,     // Tweak @TODO
}


impl MuscleConfigCommon{
    pub fn new( s_file: &str ) -> MuscleConfigCommon{

        let conf = match Ini::from_file( s_file ){
            Ok( a ) => a,
            Err ( e ) => panic!("Configuration file `{}` not found or not accessible: {:?}", s_file, e)
        };

        let s_err = format!("Configuration file `{}` is missing this entry: ", s_file);

//        let context_prefixes:Option<Vec<String>> = conf.get_vec("Common-Webservice", "active-contexts");
        let context_prefixes:Vec<String> = conf.get_vec("Common-Webservice", "active_contexts").unwrap();
//        let mut contexts = HashMap::new();
        let contexts = context_prefixes.iter().map( | prefix | {
            ( prefix.to_owned(),
            MuscleConfigContext{
                prefix: prefix.to_owned(),

                db: conf.get(&format!("{}_Database", prefix),"db").expect(
                    &format!("{}`db` in section `{}_Database`", s_err, prefix)[..]),

                db_user: conf.get(&format!("{}_Database", prefix), "db_user").expect(
                    &format!("{}`db_user` in section `{}Database`", &s_err, prefix)[..]),
                    
                db_pass: conf.get(&format!("{}_Database", prefix), "db_pass").expect(
                    &format!("{}`db_pass` in section `{}_Database`", s_err, prefix)[..]),

                static_404_default: conf.get(&format!("{}_Webservice", prefix), "static_404_default").expect(
                    &format!("{}`static_404_default` in section `{}_Webservice`", s_err, prefix)[..]),

                pg_service_prefix: conf.get(&format!("{}_Webservice", prefix), "pg_service_prefix").expect(
                    &format!("{}`pg_service_prefix` in section `{}_Webservice`", s_err, prefix)[..]),

                index_file: conf.get(&format!("{}_Webservice", prefix), "index_file").expect(
                    &format!("{}`index_file` in section `{}_Webservice`", s_err, prefix)[..]),

                token_name: conf.get(&format!("{}_Authorization", prefix), "pg_token_name").expect(
                    &format!("{}`pg_token_name` in section `{}_Authorization`", s_err, prefix)[..]),

                token_secret: conf.get(&format!("{}_Authorization", prefix), "pg_token_secret").expect(
                    &format!("{}`pg_token_secret` in section `{}_Authorization`", s_err, prefix)[..]),

                pg_setvar_prefix: conf.get(&format!("{}_Authorization", prefix), "pg_setvar_prefix").expect(
                    &format!("{}`pg_setvar_prefix` in section `{}_Authorization`", s_err, prefix)[..]),

                use_eq_syntax_on_url_parameters: conf.get(&format!("{}_API", prefix), "api_use_eq_syntax_on_url_parameters").expect(
                    &format!("{}`api_use_eq_syntax_on_url_parameters` in section `{}_API`", s_err, prefix)[..]),

                api_conf: conf.get(&format!("{}_API", prefix), "api_conf").expect(
                    &format!("{}`api_conf` in section `{}_API`", s_err, prefix)[..]),


                dynamic_err: conf.get(&format!("{}_API", prefix), "dynamic_err").expect(
                    &format!("{}`dynamic_err` in section `{}_API`", s_err, prefix)[..]),

                }
            )
        }).collect();
        MuscleConfigCommon{
                
            timezone: conf.get("Database", "timezone").expect(
                &format!("{}{}", s_err, "`timezone` in section `Database`")[..]),

            port: conf.get("Common-Webservice", "port").expect(
                &format!("{}{}", s_err, "`port` in section `Webservice`")[..]),

            addr: conf.get("Common-Webservice", "addr").expect(
                &format!("{}{}", s_err, "`addr` in section `Webservice`")[..]),

            client_ip_allow: conf.get("Common-Webservice", "client_ip_allow").expect(
                &format!("{} `client_ip_allow` in section `Common-Webservice`", s_err)[..]),

            server_read_timeout_ms: conf.get("Common-Webservice", "server_read_timeout_ms").expect(
                &format!("{}{}", s_err, "`server_read_timeout_ms` in section `Webservice`")[..]),
                
            server_read_chunksize: conf.get("Common-Webservice", "server_read_chunksize").expect(
                &format!("{}{}", s_err, "`server_read_chunksize` in section `Webservice`")[..]),

            server_use_https: conf.get("Common-Webservice", "https").expect(
                &format!("{}{}", s_err, "`https` in section `Webservice`")[..]),

            cert_pass: conf.get("Common-Webservice", "cert_pass").expect(
                &format!("{}{}", s_err, "`cert_pass` in section `Webservice`")[..]),

            cert_file: conf.get("Common-Webservice", "cert_file").expect(
                &format!("{}{}", s_err, "`cert_file` in section `Webservice`")[..]),

            contexts: contexts,

        }
    }
}
