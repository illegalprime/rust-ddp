
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
