extern crate rustc_serialize;
extern crate websocket;

use std::io::Write;
use std::io::Read;
use rustc_serialize::json::Json;
use rustc_serialize::json;
use websocket::Message;
use websocket::client::request::Url;
use websocket::dataframe::DataFrame;
use websocket::client::sender::Sender;
use websocket::client::receiver::Receiver;
use websocket::stream::WebSocketStream;
use websocket::result::WebSocketError;

pub use websocket::result::WebSocketResult;

mod requests;
mod responses;

use responses::*;
use requests::*;

type Client = websocket::Client<DataFrame, Sender<WebSocketStream>, Receiver<WebSocketStream>>;


fn handshake(url: &Url) -> WebSocketResult<Client> {
    // Handshake with the server
    let knock  = try!( Client::connect(url) );
    let answer = try!( knock.send() );
    try!( answer.validate() );

    // Get referennce to the client
    Ok(answer.begin())
}

fn negotiate(client: &mut Client, version: &str) -> WebSocketResult<NegotiateResp> {
    let request = requests::Connect::new(version);
    let request = Message::Text(request.to_json());

    try!( client.send_message(request) );

    for msg_result in client.incoming_messages::<Message>() {
        if let Ok(Message::Text(plaintext)) = msg_result {
            if let Ok(success) = json::decode::<VersionSuccess>(&plaintext) {
                if success.msg == "connected" {
                    return Ok(NegotiateResp::SessionId(success.session));
                }
            } else if let Ok(failure) = json::decode::<VersionFailed>(&plaintext) {
                if failure.msg == "failed" {
                    return Ok(NegotiateResp::Version(failure.version));
                }
            }
        }
    }
    Err(WebSocketError::ResponseError("No protocol version reply from the server.".to_string()))
}

pub enum NegotiateResp {
    SessionId(String),
    Version(String),
}

fn connect(url: &Url) -> WebSocketResult<(Client, String)> {
    let mut client = try!( handshake(url) );
    let mut curr_version = VER_NOW;

    loop {
        match negotiate(&mut client, curr_version) {
            Err(e) => return Err(e),
            Ok(NegotiateResp::SessionId(session)) => return Ok((client, session)),
            Ok(NegotiateResp::Version(version))   => {
                if let Some(v) = VERSIONS.iter().find(|&v| *v == version) {
                    curr_version = v;
                } else {
                    return Err(WebSocketError::ResponseError("No matching version.".to_string()));
                }
            },
        };
    }
}

#[test]
fn it_works() {

    let url = Url::parse("ws://127.0.0.1:3000/websocket").unwrap(); // Get the URL
    // assert_eq!("{:?}", format!("{:?}", request));

    if let Ok((client, session_id)) = connect(&url) {
        println!("The session id is: {}", session_id);
    }

    // let request = Client::connect(url).ok().expect("Could not connect"); // Connect to the server
    // let response = request.send().ok().expect("Could not send response."); // Send the request
    //
    //
    // response.validate().ok().expect("response is not valid!!"); // Ensure the response is valid
    //
    // let mut client = response.begin(); // Get a Client
    // let body = "{
    //     \"msg\":     \"connect\",
    //     \"version\": \"1\",
    //     \"support\": [\"1\", \"pre2\", \"pre1\"]
    // }".to_string();
    //
    // let message = Message::Text(body);
    // client.send_message(message).ok().unwrap(); // Send message
    //
    // for message in client.incoming_messages::<Message>() {
    //
    //     let json_text = match message.ok().unwrap() {
    //         Message::Text(plain) => plain,
    //         _ => unreachable!(),
    //     };
    //
    //     let json = Json::from_str(&json_text);
    //
    //     let object = match json.ok().unwrap() {
    //         Json::Object(o) => o,
    //         _ => unreachable!(),
    //     };
    //
    //     let message = match object.get("msg") {
    //         Some(m) => match m.clone() {
    //                         Json::String(s) => s,
    //                         _ => unreachable!(),
    //                    },
    //         None    => "No Message".to_string(),
    //     };
    //
    //     println!("The message was: {}", message);
    // }
}
