use tini::Ini;
use std::net::Ipv4Addr;

// ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
// MuscleConfiguration parameters
#[derive (Debug)]
pub struct MuscleConfig{
    pub port: usize,                     // Server port
    pub addr: String,                    // Server address
    pub db: String,                      // Name of Postgres (Pg) db
    pub db_user: String,                 // Name of Pg user
    pub db_pass: String,                 // Password of Pg user
    pub cert_pass: String,               // Pwd for server certificate (TLS/Https)
    pub cert_file: String,               // Certificate file (TLS/Https)
    pub api_conf: String,                // OpenAPI config file containing endpoints
    pub static_files_folder: String, // Path to serve static files from
    pub token_name: String,              // Pg token name: @TODO
    pub token_secret: String,            // Pg shared token secret: @TODO
    pub pg_setvar_prefix: String,        // Pg prefix for variables that are set in postgres through the token: @TODO
    pub timezone: String,                // Timezone to set Pg to
    pub static_404_default: String,      // Default Err page for "not found" -- none if set to "none"
    pub dynamic_err: String,             // Default Err JSON msg for errors in dynamic requests (or "none", meaning detailed error messages will be returned instead)
    pub index_file: String,              // File to return if a folder is requested (or "none")
    pub server_read_timeout_ms: u64,     // Tweak @TODO
    pub server_read_chunksize: usize,     // Tweak @TODO
    pub server_use_https: bool,           // Listen for https requests (true) or http?
    pub client_ip_allow: Ipv4Addr,        //
    pub use_eq_syntax_on_url_parameters: bool // translate https://url?param=eq.5 to "param=5" (...lt.5 to "param < 5"). @TODO. true not yet implemented (August 24, 21)
}

impl MuscleConfig{
    pub fn new( s_file: &str ) -> MuscleConfig{

        let conf = match Ini::from_file( s_file ){
            Ok( a ) => a,
            Err ( e ) => panic!("Configuration file `{}` not found or not accessible: {:?}", s_file, e)
        };

        let s_err = format!("Configuration file `{}` is missing this entry: ", s_file);

        MuscleConfig{
            db: conf.get("Database", "db").expect(
                &format!("{}{}", s_err, "`db` in section `Database`")[..]),

            db_user: conf.get("Database", "db_user").expect(
                &format!("{}{}", s_err, "`db_user` in section `Database`")[..]),
                
            db_pass: conf.get("Database", "db_pass").expect(
                &format!("{}{}", s_err, "`db_pass` in section `Database`")[..]),
                
            timezone: conf.get("Database", "timezone").expect(
                &format!("{}{}", s_err, "`timezone` in section `Database`")[..]),

            port: conf.get("Webservice", "port").expect(
                &format!("{}{}", s_err, "`port` in section `Webservice`")[..]),

            addr: conf.get("Webservice", "addr").expect(
                &format!("{}{}", s_err, "`addr` in section `Webservice`")[..]),

            server_read_timeout_ms: conf.get("Webservice", "server_read_timeout_ms").expect(
                &format!("{}{}", s_err, "`server_read_timeout_ms` in section `Webservice`")[..]),
                
            server_read_chunksize: conf.get("Webservice", "server_read_chunksize").expect(
                &format!("{}{}", s_err, "`server_read_chunksize` in section `Webservice`")[..]),

            server_use_https: conf.get("Webservice", "https").expect(
                &format!("{}{}", s_err, "`https` in section `Webservice`")[..]),

            client_ip_allow: conf.get("Webservice", "client_ip_allow").expect(
                &format!("{}{}", s_err, "`https` in section `Webservice`")[..]),

            cert_pass: conf.get("Webservice", "cert_pass").expect(
                &format!("{}{}", s_err, "`cert_pass` in section `Webservice`")[..]),

            cert_file: conf.get("Webservice", "cert_file").expect(
                &format!("{}{}", s_err, "`cert_file` in section `Webservice`")[..]),

            api_conf: conf.get("Webservice", "api_conf").expect(
                &format!("{}{}", s_err, "`api_conf` in section `Webservice`")[..]),

            static_files_folder: conf.get("Webservice", "static_files_folder").expect(
                &format!("{}{}", s_err, "`static_files_folder` in section `Webservice`")[..]),

            token_name: conf.get("Authorization", "pg_token_name").expect(
                &format!("{}{}", s_err, "`pg_token_name` in section `Authorization`")[..]),

            token_secret: conf.get("Authorization", "pg_token_secret").expect(
                &format!("{}{}", s_err, "`pg_token_secret` in section `Authorization`")[..]),

            pg_setvar_prefix: conf.get("Authorization", "pg_setvar_prefix").expect(
                &format!("{}{}", s_err, "`pg_setvar_prefix` in section `Authorization`")[..]),

            use_eq_syntax_on_url_parameters: conf.get("Service", "api_use_eq_syntax_on_url_parameters").expect(
                &format!("{}{}", s_err, "`api_use_eq_syntax_on_url_parameters` in section `Service`")[..]),

            static_404_default: conf.get("Service", "static_404_default").expect(
                &format!("{}{}", s_err, "`static_404_default` in section `Service`")[..]),

            dynamic_err: conf.get("Service", "dynamic_err").expect(
                &format!("{}{}", s_err, "`dynamic_err` in section `Service`")[..]),

            index_file: conf.get("Service", "index_file").expect(
                &format!("{}{}", s_err, "`index_file` in section `Service`")[..])
        }
    }
}