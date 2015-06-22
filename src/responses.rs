use rustc_serialize::json;

#[derive(RustcDecodable)]
pub struct VersionSuccess {
    pub msg: String,
    pub session: String,
}

#[derive(RustcDecodable)]
pub struct VersionFailed {
    pub msg: String,
    pub version: String,
}

#[derive(RustcDecodable)]
pub struct Ping {
    pub msg: String,
    pub id:  Option<String>,
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

impl FromResponse for VersionFailed {
    fn from_response(response: &str) -> Option<Self> {
        if let Ok(decoded) = json::decode::<VersionFailed>(response) {
            if decoded.msg == "failed" {
                return Some(decoded);
            }
        }
        None
    }
}

impl FromResponse for VersionSuccess {
    fn from_response(response: &str) -> Option<Self> {
        if let Ok(decoded) = json::decode::<VersionSuccess>(response) {
            if decoded.msg == "connected" {
                return Some(decoded);
            }
        }
        None
    }
}
