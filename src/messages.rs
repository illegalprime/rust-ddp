use rustc_serialize::json;

pub const VERSIONS: &'static [&'static str; 3] = &["1", "pre2", "pre1"];

#[derive(RustcEncodable)]
pub struct Connect {
    msg: &'static str,
    version: &'static str,
    support: &'static [&'static str],
}

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

#[derive(RustcEncodable)]
pub struct Pong {
    pub msg: &'static str,
    pub id:  Option<String>,
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

pub trait FromResponse {
    fn from_response(response: &str) -> Option<Self>;
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