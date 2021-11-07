use crate::Response;
use std::time::Instant;
use std::time::Duration;
use log::{debug, info};
use std::collections::HashMap;

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
                debug!("Cache: size limit of {} MB reached, trying purge before adding new response.", self.size_limit);
                self.purge_expired_responses();
            }
            if self.size + r_size > self.size_limit{
                info!("Cache: limit of {} MB still exceeded after purge, cannot cache `{}`", self.size_limit, &url);
            }else{
                self.size = self.size + r_size;
                debug!("Cache: adding `{}`, size is now: {} MB", url, self.get_size_mb());
                self.cache.insert(url, ResponseCacheElement::new( r ) );
            }
        }else{
            debug!("Cache: no add, already present: `{}`", url);
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
                        debug!("Cache: removing `{}` because expired.", &resp.0);
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
    pub fn get( &mut self, url: &str ) -> Option<Response>{
        match self.cache.get( url ){
            Some( resp ) => {
                if resp.is_expired(){
                    debug!("Cache: expired response to `{}`.", url);
                    self.drop( url );
                    None
                }else{
                    debug!("Cache: retrieved `{}`.", url);
                    Some( resp.cached_response.clone() )
                }
            },
            _ => {
                debug!("Cache: not in cache: `{}`", url);
                None
            }
        }
    }
}
