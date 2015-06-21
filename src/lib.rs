extern crate rustc_serialize;
extern crate websocket;

use std::io::Write;

use rustc_serialize::json::Json;

use websocket::{Client, Message};
use websocket::client::request::Url;

#[test]
fn it_works() {
    let url = Url::parse("ws://127.0.0.1:3000/websocket").unwrap(); // Get the URL
    // assert_eq!("{:?}", format!("{:?}", request));

    let request = Client::connect(url).ok().expect("Could not connect"); // Connect to the server
    let response = request.send().ok().expect("Could not send response."); // Send the request


    response.validate().ok().expect("response is not valid!!"); // Ensure the response is valid

    let mut client = response.begin(); // Get a Client
    let body = "{
        \"msg\":     \"connect\",
        \"version\": \"1\",
        \"support\": [\"1\", \"pre2\", \"pre1\"]
    }".to_string();

    let message = Message::Text(body);
    client.send_message(message).ok().unwrap(); // Send message

    for message in client.incoming_messages::<Message>() {

        let json_text = match message.ok().unwrap() {
            Message::Text(plain) => plain,
            _ => unreachable!(),
        };

        let json = Json::from_str(&json_text);

        let object = match json.ok().unwrap() {
            Json::Object(o) => o,
            _ => unreachable!(),
        };

        let message = match object.get("msg") {
            Some(m) => match m.clone() {
                            Json::String(s) => s,
                            _ => unreachable!(),
                       },
            None    => "No Message".to_string(),
        };

        println!("The message was: {}", message);
    }
}
