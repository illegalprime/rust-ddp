use rustc_serialize::json;
use std::fmt::Display;
use std::fmt::Formatter;
use std::fmt::Error as FmtError;

pub const VERSIONS: &'static [&'static str; 3] = &["1", "pre2", "pre1"];
pub type Ejson = json::Json;

/***************************
 *        Responses        *
 ***************************/

#[derive(RustcDecodable)]
pub struct Connected {
    pub msg: String,
    pub session: String,
}

#[derive(RustcDecodable)]
pub struct Failed {
    pub msg: String,
    pub version: String,
}

#[derive(RustcDecodable)]
pub struct Ping {
    pub msg: String,
    pub id:  Option<String>,
}

#[derive(RustcDecodable)]
pub struct MethodResult {
    pub msg:    String,
    pub id:     String,
    pub result: String,
}

/***************************
 *        Requests         *
 ***************************/

#[derive(RustcEncodable)]
pub struct Connect {
    msg: &'static str,
    version: &'static str,
    support: &'static [&'static str],
}

pub struct Pong;

pub struct Method;

pub struct Subscribe;

pub struct Unsubscribe;

pub struct Minify<'a>(&'a Vec<&'a json::Json>);

impl Pong {
    pub fn text<'l>(id: Option<&'l str>) -> String {
        if let Some(id) = id {
            format!("{{\"msg\":\"pong\",\"id\":{}}}", id)
        } else {
            "{\"msg\":\"pong\"}".to_string()
        }
    }
}

impl Method {
    pub fn text<'l>(id: &'l str, method: &'l str, params: Option<&Vec<&Ejson>>) -> String {
        if let Some(args) = params {
            format!("{{\"msg\":\"method\",\"id\":\"{}\",\"method\":\"{}\",\"params\":{}}}", &id, method, Minify(args))
        } else {
            format!("{{\"msg\":\"method\",\"id\":\"{}\",\"method\":\"{}\"}}", &id, method)
        }
    }
}

impl Subscribe {
    pub fn text<'l>(id: &'l str, name: &'l str, params: Option<&Vec<&Ejson>>) -> String {
        if let Some(args) = params {
            format!("{{\"msg\":\"sub\",\"id\":\"{}\",\"name\":\"{}\",\"params\":{}}}", &id, name, Minify(args))
        } else {
            format!("{{\"msg\":\"sub\",\"id\":\"{}\",\"name\":\"{}\"}}", &id, name)
        }
    }
}

impl Unsubscribe {
    pub fn text<'l>(id: &'l str) -> String {
        format!("{{\"msg\":\"unsub\",\"id\":\"{}\"}}", &id)
    }
}

impl Connect {
    pub fn new(version: &'static str) -> Self {
        Connect {
            msg: "connect",
            version: version,
            support: VERSIONS,
        }
    }
}

impl<'a> Display for Minify<'a> {
    fn fmt(&self, formatter: &mut Formatter) -> Result<(), FmtError> {
        try!( formatter.write_str("[") );
        if let Some(first) = self.0.get(0) {
            try!( formatter.write_fmt(format_args!("{}", first)) );
            for json in self.0.iter().skip(1) {
                try!( formatter.write_fmt(format_args!(",{}", json)) );
            }
        }
        formatter.write_str("]")
    }
}

pub trait FromResponse {
    fn from_response(response: &str) -> Option<Self>;
}

impl FromResponse for MethodResult {
    fn from_response(response: &str) -> Option<Self> {
        if let Ok(decoded) = json::decode::<MethodResult>(response) {
            if decoded.msg == "result" {
                return Some(decoded);
            }
        }
        None
    }
}

impl FromResponse for Ping {
    fn from_response(response: &str) -> Option<Self> {
        if let Ok(decoded) = json::decode::<Ping>(response) {
            if decoded.msg == "ping" {
                return Some(decoded);
            }
        }
        None
    }
}

impl FromResponse for Failed {
    fn from_response(response: &str) -> Option<Self> {
        if let Ok(decoded) = json::decode::<Failed>(response) {
            if decoded.msg == "failed" {
                return Some(decoded);
            }
        }
        None
    }
}

impl FromResponse for Connected {
    fn from_response(response: &str) -> Option<Self> {
        if let Ok(decoded) = json::decode::<Connected>(response) {
            if decoded.msg == "connected" {
                return Some(decoded);
            }
        }
        None
    }
}
