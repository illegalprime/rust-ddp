extern crate rustc_serialize;
extern crate websocket;

use rustc_serialize::json;
use rustc_serialize::Encodable;
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

pub struct DdpClient {
    client: Client,
    session_id: String,
    version: &'static str,
}

impl DdpClient {
    pub fn new(url: &Url) -> Result<Self, DdpConnError> {
        let (client, session_id, v_index) = try!( DdpClient::connect(url) );
        Ok(DdpClient {
            client:     client,
            session_id: session_id,
            version:    VERSIONS[v_index],
        })
    }

    fn handshake(url: &Url) -> Result<Client, DdpConnError> {
        // Handshake with the server
        let knock  = try!( Client::connect(url).map_err(|e| DdpConnError::Network(e)) );
        let answer = try!( knock.send()        .map_err(|e| DdpConnError::Network(e)) );
        try!( answer.validate()                .map_err(|e| DdpConnError::Network(e)) );

        // Get referennce to the client
        Ok(answer.begin())
    }

    fn negotiate(client: &mut Client, version: &str) -> Result<NegotiateResp, DdpConnError> {
        let request = requests::Connect::new(version);
        let request = Message::Text(request.to_json());

        try!( client.send_message(request).map_err(|e| DdpConnError::Network(e)) );

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
        Err(DdpConnError::NoVersionFromServer)
    }

    fn connect(url: &Url) -> Result<(Client, String, usize), DdpConnError> {
        let mut client = try!( DdpClient::handshake(url) );
        let mut version = VER_NOW;
        let mut v_index = 0;

        loop {
            match DdpClient::negotiate(&mut client, version) {
                Err(e) => return Err(e),
                Ok(NegotiateResp::SessionId(session)) => return Ok((client, session, v_index)),
                Ok(NegotiateResp::Version(server_version))   => {
                    // TODO: Maybe this should be faster, maybe its enough.
                    let found = VERSIONS.iter().enumerate().find(|&(_, &v)| *v == server_version);
                    if let Some((i, &v)) = found {
                        v_index = i;
                        version = v;
                    } else {
                        return Err(DdpConnError::NoMatchingVersion);
                    }
                },
            };
        }
    }

    pub fn send_raw<E>(&mut self, message: &E) where E: Encodable {
        // TODO: Unsafe
        let message = json::encode(message).unwrap();
        let message = Message::Text(message);
        self.client.send_message(message).ok();
    }

    pub fn session(&self) -> &str {
        &self.session_id
    }

    pub fn version(&self) -> &str {
        &self.version
    }
}

pub enum NegotiateResp {
    SessionId(String),
    Version(String),
}

#[derive(Debug)]
pub enum DdpConnError {
    Network(WebSocketError),
    NoVersionFromServer,
    NoMatchingVersion,
}


#[test]
fn test_connect_version() {
    let url = Url::parse("ws://127.0.0.1:3000/websocket").unwrap(); // Get the URL

    let ddp_client_result = DdpClient::new(&url);

    match ddp_client_result {
        Ok(client) => println!("The session id is: {} with DDP v{}", client.session(), client.version()),
        Err(err)   => panic!("An error occured: {:?}", err),
    };
}
