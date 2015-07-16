use rustc_serialize::json;

pub const VERSIONS: &'static [&'static str; 3] = &["1", "pre2", "pre1"];
pub type Ejson = String;

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
    pub error:  String,
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

impl Pong {
    pub fn text(id: Option<String>) -> String {
        if let Some(id) = id {
            format!("{{\"msg\":\"pong\",\"id\":{}}}", id)
        } else {
            "{\"msg\":\"pong\"}".to_string()
        }
    }
}

impl Method {
    pub fn text<'l>(method: &'l str, params: Option<Vec<Ejson>>) -> (String, String) {
        // TODO: Make a real random ID.
        let id = "R8nXmEpHtpMfi6xJZ".to_string();
        if let Some(args) = params {
            (format!("{{\"msg\":\"method\",\"id\":\"{}\",\"method\":\"{}\",\"params\":{:?}}}", &id, method, args), id)
        } else {
            (format!("{{\"msg\":\"method\",\"id\":\"{}\",\"method\":\"{}\"}}", &id, method), id)
        }
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
