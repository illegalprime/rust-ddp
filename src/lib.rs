#[macro_use]
extern crate log;
extern crate rustc_serialize;
extern crate websocket;

use std::borrow::Cow;
use std::collections::hash_map::HashMap;
use std::sync::{Arc, Mutex};
use std::sync::mpsc::channel;
use std::sync::mpsc::Sender as AtomicSender;
use std::thread;
use std::thread::JoinHandle;
// use rand::Rng;
use rustc_serialize::json;
use rustc_serialize::json::Json;
use rustc_serialize::json::Object;
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

mod random;
use random::Random;

mod collections;
use collections::MongoCallbacks;
use collections::Subscriptions;

pub use collections::MongoCollection;
pub use collections::ListenerId;

type Client = websocket::Client<DataFrame, sender::Sender<WebSocketStream>, receiver::Receiver<WebSocketStream>>;
type MethodCallback = Box<FnMut(Result<&Ejson, &Ejson>) + Send + 'static>;

pub struct OnPanic(Arc<Fn() + Sync + Send>);

impl Drop for OnPanic {
    fn drop(&mut self) {
        self.0();
    }
}

pub struct Methods {
    outgoing:        Arc<Mutex<AtomicSender<String>>>,
    pending_methods: HashMap<String, MethodCallback>,
    rng: Random,
}

impl Methods {
    fn new(outgoing: Arc<Mutex<AtomicSender<String>>>) -> Self {
        Methods {
            rng:             Random::new(),
            pending_methods: HashMap::new(),
            outgoing:        outgoing,
        }
    }

    fn send<C>(&mut self, method: &str, params: Option<&Vec<&Ejson>>, callback: C)
    where C: FnMut(Result<&Ejson, &Ejson>) + Send + 'static {
        let id = self.rng.id();
        let method = Method::text(&id, method, params);
        outgoing.lock().unwrap().send(method).unwrap();
        self.pending_methods.insert(id, Box::new(callback));
    }

    fn apply(&mut self, id: &str, response: Result<&Ejson, &Ejson>) {
        if let Some(method) = self.pending_methods.remove(id) {
            let mut method: MethodCallback = method;
            method(response);
        }
    }
}

trait Reply<'a> {
    #[inline]
    fn id(&'a self) -> Option<&'a str> {
        self.get_ejson("id").and_then(|id| id.as_string())
    }

    #[inline]
    fn collection(&'a self) -> Option<&'a str> {
        self.get_ejson("collection").and_then(|c| c.as_string())
    }

    #[inline]
    fn fields(&'a self) -> Option<&'a Ejson> {
        self.get_ejson("fields")
    }

    #[inline]
    fn cleared(&'a self) -> Option<&'a Ejson> {
        self.get_ejson("cleared")
    }

    #[inline]
    fn error(&'a self) -> Option<&'a Ejson> {
        self.get_ejson("error")
    }

    #[inline]
    fn result(&'a self) -> Option<&'a Ejson> {
        self.get_ejson("result")
    }

    #[inline]
    fn subs(&'a self) -> Option<&'a Ejson> {
        self.get_ejson("subs")
    }

    fn get_ejson(&'a self, &str) -> Option<&'a Json>;
}

impl<'a> Reply<'a> for Object {
    #[inline]
    fn get_ejson(&'a self, key: &str) -> Option<&'a Ejson> {
        self.get(key)
    }
}

#[derive(Clone)]
pub struct Core {
    methods:    Arc<Mutex<Methods>>,
    mongos:     Arc<Mutex<HashMap<String, Arc<Mutex<MongoCallbacks>>>>>,
    subs:       Arc<Mutex<Subscriptions>>,
    transfer:   Arc<Mutex<AtomicSender<String>>>,
}

impl Core {
    fn handle_ping(&self, message: &Object) {
        self.transfer.lock().unwrap().send(Pong::text(message.id())).unwrap();
    }

    fn handle_result(&self, message: &Object) {
        if let Some(id) = message.id() {
            let result = match (message.get("error"), message.get("result")) {
                (Some(e), None)    => Err(e),
                (None,    Some(r)) => Ok(r),
                _                  => return,
            };
            self.methods.lock().unwrap().apply(id, result);
        }
    }

    fn handle_added(&self, message: &Object) {
        let collection = self.collection(message);
        let id = message.id();

        if let (Some(id), Some(mongo)) = (id, collection) {
            let fields = message.fields();
            mongo.lock().unwrap().notify_insert(id, fields);
        }
    }

    fn handle_changed(&self, message: &Object) {
        let collection = self.collection(message);
        let id = message.id();

        if let (Some(id), Some(mongo)) = (id, collection) {
            let fields = message.fields();
            let cleared = message.cleared();
            mongo.lock().unwrap().notify_change(id, fields, cleared);
        }
    }

    fn handle_removed(&self, message: &Object) {
        let collection = self.collection(message);
        let id = message.id();

        if let (Some(id), Some(mongo)) = (id, collection) {
            mongo.lock().unwrap().notify_remove(id);
        }
    }

    fn handle_ready(&self, message: &Object) {
        let ids = message.subs().and_then(|s| s.as_array()).and_then(|a| {
            let idies: Vec<&str> = a.iter()
                .map(|id| id.as_string())
                .filter(|o| o.is_some())
                .map(|s| s.unwrap())
                .collect();
            Some(idies)
        });
        if let Some(ids) = ids {
            self.subs.lock().unwrap().notify(Ok(ids));
        }
    }

    fn handle_nosub(&self, message: &Object) {
        let id = message.id();
        let error = message.error();
        if let (Some(error), Some(id)) = (error, id) {
            self.subs.lock().unwrap().notify(Err((id, error)));
        }
    }

    #[inline]
    fn collection(&self, message: &Object) -> Option<&Arc<Mutex<MongoCallbacks>>> {
        message.collection().and_then(|c| {
            self.mongos.lock().unwrap().get(c)
        });
    }
}

pub struct Connection<'s> {
    core:       Core,
    receiver:   JoinHandle<()>,
    sender:     JoinHandle<()>,
    session_id: String,
    version:    &'static str,
}

impl<'s> Connection<'s> {
    pub fn new<F>(url: &Url, on_crash: F) -> Result<Self, DdpConnError>
    where F: Fn() + Sync + Send + 'static {
        let (client, session_id, v_index) = try!( Connection::connect(url) );
        let (mut sender, mut receiver) = client.split();
        let report = Arc::new(on_crash);
        let (report_sending, report_receiving) = (report.clone(), report);

        let (tx, rx) = channel();
        let tx      = Arc::new(Mutex::new(tx));
        let methods = Arc::new(Mutex::new(Methods::new(tx.clone())));
        let mongos  = Arc::new(Mutex::new(HashMap::new()));
        let subs    = Arc::new(Mutex::new(Subscriptions::new(tx.clone())));

        let core = Core {
            methods:  methods,
            mongos:   mongos,
            subs:     subs,
            transfer: tx,
        };
        let client_core = core.clone();

        let receiving = thread::spawn(move || {
            OnPanic(report_receiving);
            let handlers: HashMap<&'static str, &Fn(&Core, &Object)> = HashMap::new();

            handlers.insert("ping",    &Core::handle_ping);
            handlers.insert("result",  &Core::handle_result);
            handlers.insert("added",   &Core::handle_added);
            handlers.insert("changed", &Core::handle_changed);
            handlers.insert("removed", &Core::handle_removed);
            handlers.insert("ready",   &Core::handle_ready);
            handlers.insert("nosub",   &Core::handle_nosub);

            for packet in receiver.incoming_messages() {
                let text = match packet {
                    Ok(Message::Text(m))  => m,
                    _ => continue,
                };

                let data = Json::from_str(&text).ok().and_then(|o| o.as_object());
                let message = data
                    .and_then(|o| o.get("msg"))
                    .and_then(|m| m.as_string());

                if let (Some(message), Some(data)) = (message, data) {
                    if let Some(handler) = handlers.get(message) {
                        handler(&core, data);
                    }
                }
            }
        });

        let sending = thread::spawn(move || {
            OnPanic(report_sending);
            while let Ok(message) = rx.recv() {
                sender.send_message(Message::Text(message)).unwrap();
            }
        });

        Ok(Connection {
            core:       client_core,
            session_id: session_id,
            version:    VERSIONS[v_index],
            receiver:   receiving,
            sender:     sending,
        })
    }

    pub fn call<C>(&self, method: &str, params: Option<&Vec<&Ejson>>, callback: C)
    where C: FnMut(Result<&Ejson, &Ejson>) + Send + 'static {
        self.core.methods.lock().unwrap().send(method, params, callback);
    }

    pub fn mongo<S>(&self, collection: S) -> &'s Collection
    where S: Into<String> {
        let collection = collection.into();
        let callbacks = self.core.mongos.lock().unwrap().entry(collection.clone()).or_insert_with(|| {
            Arc::new(Mutex::new(MongoCallbacks::new(collection, self.core.clone())));
        });
        &callbacks
    }

    pub fn session(&self) -> &str {
        &self.session_id
    }

    pub fn version(&self) -> &'static str {
        &self.version
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

        if let Ok(Message::Text(plaintext)) = client.incoming_messages().next() {
            if let Some(message) = Json::from_str(plaintext).ok().and_then(|o| o.as_object()) {
                match message.get("msg") {
                    Some("success") => {
                        if let Some(session) = message.get("session").and_then(|v| v.as_string()) {
                            return Ok(NegotiateResp::SessionId(session));
                        }
                    },
                    Some("failure") => {
                        if let Some(version) = message.get("version").and_then(|v| v.as_string()) {
                            return Ok(NegotiateResp::Version(version));
                        }
                    }
                }
            }
        }
        Err(DdpConnError::MalformedPacket)
    }

    fn connect(url: &Url) -> Result<(Client, String, usize), DdpConnError> {
        let mut client = try!( Connection::handshake(url) );
        let mut v_index = 0;

        loop {
            // TODO: (BUG) Connection gets closed after if versions do not match
            // we need to open it up again.
            match Connection::negotiate(&mut client, VERSIONS[v_index]) {
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
}

pub enum NegotiateResp {
    SessionId(String),
    Version(String),
}

#[derive(Debug)]
pub enum DdpConnError {
    Network(WebSocketError),
    MalformedPacket,
    NoMatchingVersion,
}

#[test]
fn test_connect_version() {
    let url = Url::parse("ws://127.0.0.1:3000/websocket").unwrap(); // Get the URL
    let ddp_client_result = Connection::new(&url);

    let client = match ddp_client_result {
        Ok(client) => client,
        Err(err)   => panic!("An error occured: {:?}", err),
    };

    println!("The session id is: {} with DDP v{}", client.session(), client.version());

    println!("\n\nCalling a real method!\n\n");
    client.call("hello", None, |result| {
        print!("Ran method, ");
        match result {
            Ok(output) => println!("got a result: {}", output),
            Err(error) => println!("got an error: {}", error),
        }
    });

    println!("\n\nCalling a fake method!\n\n");
    client.call("not_a_method", None, |result| {
        print!("Ran method, ");
        match result {
            Ok(output) => println!("got a result: {}", output),
            Err(error) => println!("got an error: {}", error),
        }
    });

    println!("\n\nSubscribing to MongoColl!\n\n");
    let mongo = client.mongo("MongoColl");
    mongo.on_add(|id, _| {
        println!("Added record with id: {}", &id);
    });

    println!("\n\nInserting a record!\n\n");
    let record = json::Json::from_str("{ \"first ever meteor data from rust\": true }").unwrap();
    mongo.insert(&record, |result| {
        match result {
            Ok(_) => println!("First every successful insertion into Meteor through rust!"),
            Err(_) =>  println!("Damn! Got an error."),
        };
    });

    println!("\n\nRemoving records...\n\n");
    mongo.remove(&record, |result| {
        println!("removed records {:?}", result);
    });

    println!("Subscribing to non existent collection.");
    let nomongo = client.mongo("SomethingElse");
    nomongo.on_ready(|result| {
        match result {
            Ok(()) => unreachable!(),
            Err(e) => println!("Got an error, this is expected: {}", e),
        }
    });
}
