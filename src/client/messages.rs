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

impl Pong {
    pub fn text<'l>(id: Option<&'l str>) -> String {
        if let Some(id) = id {
            json!({
                "msg": "pong",
                "id": id
            }).to_string()
        } else {
             json!({
                "msg": "pong"
            }).to_string()
        }
    }
}

impl Method {
    pub fn text<'l>(id: &'l str, method: &'l str, params: Option<&Vec<&Ejson>>) -> String {
        if let Some(args) = params {
             json!({
                "msg": "method",
                "id": id,
                "method": method,
                "params": args
            }).to_string()
        } else {
            json!({
                "msg": "method",
                "id": id,
                "method": method
            }).to_string()
        }
    }
}

impl Subscribe {
    pub fn text<'l>(id: &'l str, name: &'l str, params: Option<&Vec<&Ejson>>) -> String {
        if let Some(args) = params {
            json!({
                "msg": "sub",
                "id": id,
                "name": name,
                "params": args
            }).to_string()
        } else {
            json!({
                "msg": "sub",
                "id": id,
                "name": name
            }).to_string()
        }
    }
}

impl Unsubscribe {
    pub fn text<'l>(id: &'l str) -> String {
        json!({
            "msg": "unsub",
            "id": id
        }).to_string()
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
