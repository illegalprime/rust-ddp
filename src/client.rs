extern crate websocket;

use std::borrow::Cow;

use websocket::client::request::Url;

pub enum Retry {
    Linear,
    Exponential,
    Quadratic,
    Custom(Box<Fn(u32, u32) -> Option<u32>>),
}

pub enum Error {
    UrlIsNotWebsocket,
    ParseError,
}

pub struct Client<'a> {
    url:   Cow<'a, Url>,
    retry: Option<Retry>,
}

impl<'a> Client<'a> {
    fn new(url: &'a str) -> Result<Self, Error> {
        /* TODO: Turn url parse errors into our errors */
        let url = try!(Url::parse(url));

        if url.protocol == WS || url.protocol == WSS {
            Ok(Client {
                url: Cow::Owned(url),
                retry: None,
            })
        } else {
            Err(Error::UrlIsNotWebsocket)
        }
    }

    fn retry(&mut self, curve: Retry) {
        self.retry = Some(curve);
    }

    fn retry_custom<F>(&mut self, f: F) where F: Fn(u32, u32) -> Option<u32> {
        self.retry = Some(Box(f));
    }

    fn open(&mut self) -> Result<DdpConn, ConnError> {
        unimplemented!()
    }

    fn close(&mut self) {
        unimplemented!()
    }
}


const WS:  &'static str = "ws";
const WSS: &'static str = "wss";
