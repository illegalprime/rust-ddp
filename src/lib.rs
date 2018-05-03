#[macro_use]
extern crate log;
extern crate websocket;
#[macro_use] extern crate serde_derive;
extern crate serde;
#[macro_use] extern crate serde_json;

mod random;
pub mod client;
pub use client::Connection;
pub use websocket::client::Url;