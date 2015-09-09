use std::sync::Arc;

extern crate websocket;
use websocket::client::request::Url;

mod connection;
use self::connection::Connection;
pub use self::connection::{Collection, DdpConnError};

mod messages;
use self::messages::Ejson;

pub enum Retry {
    Linear,
    Exponential,
    Quadratic,
    Custom(Box<Fn(u32, u32) -> Option<u32>>),
}

pub struct Client {
    url:   Url,
    retry: Option<Retry>,
    conn:  Connection,
}

impl Client {
    pub fn new(url: Url) -> Result<Self, DdpConnError> {
        let (conn, _) = try!(Connection::new(&url, || {
            /* TODO */
        }));

        Ok(Client {
            url:   url,
            retry: None,
            conn:  conn,
        })
    }

    #[inline]
    pub fn call<C>(&self, method: &str, params: Option<&Vec<&Ejson>>, callback: C)
    where C: FnMut(Result<&Ejson, &Ejson>) + Send + 'static {
        self.conn.call(method, params, callback)
    }

    #[inline]
    pub fn mongo<S>(&self, collection: S) -> Arc<Collection>
    where S: Into<String> {
        self.conn.mongo(collection)
    }

    #[inline]
    pub fn session(&self) -> &str {
        self.conn.session()
    }

    #[inline]
    pub fn version(&self) -> &'static str {
        self.conn.version()
    }

    pub fn retry(&mut self, curve: Retry) {
        self.retry = Some(curve);
    }

    pub fn retry_custom<F>(&mut self, f: F)
    where F: Fn(u32, u32) -> Option<u32> + 'static {
        self.retry = Some(Retry::Custom(Box::new(f)));
    }

    pub fn no_retry(&mut self) {
        self.retry = None;
    }
}



