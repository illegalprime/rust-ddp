use std::fmt::Display;
use std::fmt::Formatter;
use std::fmt::Error as FmtError;

use serde_json;

pub const VERSIONS: &'static [&'static str; 3] = &["1", "pre2", "pre1"];
pub type Ejson = serde_json::Value;

/***************************
 *        Responses        *
 ***************************/

#[derive(Deserialize)]
pub struct Connected {
    pub msg: String,
    pub session: String,
}

#[derive(Deserialize)]
pub struct Failed {
    pub msg: String,
    pub version: String,
}

#[derive(Deserialize)]
pub struct Ping {
    pub msg: String,
    pub id:  Option<String>,
}

#[derive(Deserialize)]
pub struct MethodResult {
    pub msg:    String,
    pub id:     String,
    pub result: String,
}

/***************************
 *        Requests         *
 ***************************/

#[derive(Serialize)]
pub struct Connect {
    msg: &'static str,
    version: &'static str,
    support: &'static [&'static str],
}

pub struct Pong;

pub struct Method;

pub struct Subscribe;

pub struct Unsubscribe;

pub struct Minify<'a>(&'a Vec<&'a serde_json::Value>);

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

