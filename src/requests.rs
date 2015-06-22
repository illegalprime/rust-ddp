extern crate rustc_serialize;

use rustc_serialize::json;


pub const VERSIONS: &'static [&'static str; 3] = &["1", "pre2", "pre1"];
pub const VER_NOW: &'static str = VERSIONS[0];

#[derive(RustcEncodable)]
pub struct Connect<'l> {
    msg: &'static str,
    version: &'l str,
    support: &'static [&'static str],
}

impl<'l> Connect<'l> {
    pub fn new(version: &'l str) -> Self {
        Connect {
            msg: "connect",
            version: version,
            support: VERSIONS,
        }
    }

    pub fn to_json(&self) -> String {
        // JSON does not change, it will always be valid
        json::encode(self).unwrap()
    }
}
