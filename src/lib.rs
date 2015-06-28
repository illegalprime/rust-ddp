extern crate rustc_serialize;
extern crate websocket;
extern crate rand;

use std::boxed::FnBox;
use std::collections::hash_map::HashMap;
use std::sync::{Arc, Mutex};
use std::sync::mpsc::channel;
use std::sync::mpsc::Sender as AtomicSender;
use std::thread;
use std::thread::JoinHandle;
use rand::Rng;
use rustc_serialize::json;
use rustc_serialize::Encodable;
use websocket::Message;
use websocket::client::request::Url;
use websocket::dataframe::DataFrame;
use websocket::client::sender;
use websocket::client::receiver;
use websocket::ws::receiver::Receiver;
use websocket::ws::sender::Sender;
use websocket::stream::WebSocketStream;
use websocket::result::WebSocketError;

mod messages;
use messages::*;

type Client = websocket::Client<DataFrame, sender::Sender<WebSocketStream>, receiver::Receiver<WebSocketStream>>;

pub struct DdpClient<'l> {
    receiver: JoinHandle<()>,
    sender:   JoinHandle<()>,
    session_id: Arc<String>,
    outgoing:   Arc<Mutex<AtomicSender<String>>>,
    pending_methods: Arc<Mutex<HashMap<String, Box<FnBox(String, String) + 'l>>>>,
    version: &'static str,
}

impl<'l> DdpClient<'l> {
    pub fn new(url: &Url) -> Result<Self, DdpConnError> {
        let (client, session_id, v_index) = try!( DdpClient::connect(url) );
        let (mut sender, mut receiver) = client.split();

        let (tx, rx) = channel();
        let tx = Arc::new(Mutex::new(tx));
        let tx_looper = tx.clone();

        let methods = Arc::new(Mutex::new(HashMap::new()));
        let methods_looper = methods.clone();

        let receiver_loop = thread::spawn(move || {
            for message in receiver.incoming_messages() {
                let message = match message {
                    Ok(Message::Text(m))  => m,
                    // TODO: Something more meaningful should happen.
                    _ => continue,
                };

                if let Some(ping) = Ping::from_response(&message) {
                    DdpClient::send(Pong {
                        msg: "pong",
                        id:  ping.id,
                    }, &tx_looper);
                }
                else if let Some(out) = MethodResult::from_response(&message) {
                    // let methods: HashMap<String, Box<FnBox(String, String) + 'l>> = methods_looper.lock().unwrap();
                    //
                    // if let Some(callback) = methods.remove(&out.id) {
                    //
                    //     callback(out.error, out.result);
                    // }
                }
            }
        });

        let sender_loop = thread::spawn(move || {
            while let Ok(message) = rx.recv() {
                sender.send_message(Message::Text(message)).unwrap();
            }
        });

        Ok(DdpClient {
            receiver:   receiver_loop,
            sender:     sender_loop,
            session_id: Arc::new(session_id),
            outgoing:   tx,
            pending_methods: methods,
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

    fn negotiate(client: &mut Client, version: &'static str) -> Result<NegotiateResp, DdpConnError> {
        let request = Connect::new(version);
        let request = Message::Text(json::encode(&request).unwrap());

        try!( client.send_message(request).map_err(|e| DdpConnError::Network(e)) );

        for msg_result in client.incoming_messages() {
            if let Ok(Message::Text(plaintext)) = msg_result {
                if let Some(success) = Connected::from_response(&plaintext) {
                    return Ok(NegotiateResp::SessionId(success.session));
                } else if let Some(failure) = Failed::from_response(&plaintext) {
                    return Ok(NegotiateResp::Version(failure.version));
                }
            }
        }
        // TODO: This is probably unreachable
        Err(DdpConnError::NoVersionFromServer)
    }

    fn connect(url: &Url) -> Result<(Client, String, usize), DdpConnError> {
        let mut client = try!( DdpClient::handshake(url) );
        let mut v_index = 0;

        loop {
            match DdpClient::negotiate(&mut client, VERSIONS[v_index]) {
                Err(e) => return Err(e),
                Ok(NegotiateResp::SessionId(session)) => return Ok((client, session, v_index)),
                Ok(NegotiateResp::Version(server_version)) => {
                    // TODO: Maybe this should be faster, maybe its enough.
                    let found = VERSIONS.iter().enumerate().find(|&(_, &v)| *v == server_version);
                    v_index = match found {
                        Some((i, _)) => i,
                        _ => return Err(DdpConnError::NoMatchingVersion),
                    };
                },
            };
        }
    }

    pub fn call<C: FnBox(String, String) + 'l>(&mut self, method: &str, params: Option<&str>, callback: C) {
        // TODO: Make better ID.
        let id: i32 = 1;
        let id = id.to_string();

        DdpClient::send(Method {
            msg:    "method",
            id:     &id,
            method: method,
            params: params,
        }, &self.outgoing);

        self.pending_methods.lock().unwrap().insert(id, Box::new(callback));
    }

    // TODO: Add method to call with random seed

    pub fn block_until_err(self) {
        self.receiver.join().ok();
        self.sender.join().ok();
    }

    pub fn session(&self) -> &str {
        &self.session_id
    }

    pub fn version(&self) -> &'static str {
        &self.version
    }

    fn send<T: Encodable>(message: T, tx: &Arc<Mutex<AtomicSender<String>>>) {
        tx.lock().unwrap().send(json::encode(&message).unwrap()).unwrap();
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

    let client = match ddp_client_result {
        Ok(client) => client,
        Err(err)   => panic!("An error occured: {:?}", err),
    };

    println!("The session id is: {} with DDP v{}", client.session(), client.version());

    client.block_until_err();
}
