// extern crate rustc_serialize;
use serde_json;
extern crate websocket;

use std::collections::hash_map::HashMap;
use std::io;
use std::net::TcpStream;
use std::sync::{Arc, Mutex, MutexGuard};
use std::sync::mpsc::channel;
use std::sync::mpsc::Sender as AtomicSender;
use std::thread;
use std::thread::JoinHandle;
use serde_json::Value;
use websocket::client::Url;
use websocket::{ClientBuilder, Message};
use websocket::dataframe::DataFrame;
use websocket::client::sync::Client;
use websocket::message::OwnedMessage;
use websocket::ws::receiver::Receiver;
use websocket::ws::sender::Sender;
use websocket::result::WebSocketError;

use super::messages::*;

use random::Random;

type MethodCallback = Box<FnMut(Result<&Ejson, &Ejson>) + Send + 'static>;
type MongoLock<'s> = MutexGuard<'s, HashMap<String, Arc<Collection>>>;

pub struct Connection {
    core:       Core,
    session_id: String,
    version:    &'static str,
}

impl Connection {
    pub fn new<F>(url: &Url, on_crash: F) -> Result<(Self, ConnectionHandle), DdpConnError>
    where F: Fn() + Sync + Send + 'static {
        if url.scheme() != WS && url.scheme() != WSS {
            return Err(DdpConnError::UrlIsNotWebsocket);
        }
        let (client, session_id, v_index) = Connection::connect(url)?;
        let ( mut receiver, mut sender) = client.split().map_err(|e| DdpConnError::IoError(e))?;
        let sreport = Arc::new(OnDrop(Arc::new(on_crash)));
        let rreport = sreport.clone();

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
            let mut handlers: HashMap<&'static str, Box<Fn(&Core, &Value)>> = HashMap::new();

            handlers.insert("ping",    Box::new(Core::handle_ping));
            handlers.insert("result",  Box::new(Core::handle_result));
            handlers.insert("added",   Box::new(Core::handle_added));
            handlers.insert("changed", Box::new(Core::handle_changed));
            handlers.insert("removed", Box::new(Core::handle_removed));
            handlers.insert("ready",   Box::new(Core::handle_ready));
            handlers.insert("nosub",   Box::new(Core::handle_nosub));

            // while let Ok(OwnedMessage::Text(text)) = receiver.recv_message() {
            for message in receiver.incoming_messages() {
                match message {
                    Ok(OwnedMessage::Text(text)) => {
                        let decoded = serde_json::from_str(&text).ok();
                        // let data = decoded
                        // let data = decoded.as_ref().and_then(|j| j.as_object());
                        let message: Option<String> = decoded.as_ref().and_then(|data: &serde_json::Value|
                            data["msg"].as_str().and_then(|s|
                                Some(s.to_string())
                            )
                        );
                            // .and_then(|o| o.get("msg"))
                            // .and_then(|m| m.as_str());

                        if let (Some(message), Some(data)) = (message, decoded) {
                            if let Some(handler) = handlers.get(&message[..]) {
                                handler(&core, &data);
                            }
                        }},
                    _ => break
                }
            }
            sreport.consume();
        });

        let sending = thread::spawn(move || {
            while let Ok(message) = rx.recv() {
                if sender.send_message(&Message::text(message)).is_err() {
                    break;
                }
            }
            rreport.consume();
        });

        Ok((Connection {
            core:       client_core,
            session_id: session_id,
            version:    VERSIONS[v_index],
        }, ConnectionHandle {
            sending:   sending,
            receiving: receiving,
        }))
    }

    #[inline]
    pub fn call(&self, method: &str, params: Option<&Vec<&Ejson>>,
                callback: Box<FnMut(Result<&Ejson, &Ejson>) + Send + 'static>) {
        self.core.methods.lock().unwrap().send(method, params, callback);
    }

    pub fn mongo(&self, collection: String) -> Arc<Collection> {
        let mut callbacks = self.core.mongos.lock().unwrap();
        let callbacks = callbacks.entry(collection.clone()).or_insert_with(|| {
            Arc::new(Collection::new(collection, &self.core))
        });
        callbacks.clone()
    }

    pub fn session(&self) -> &str {
        &self.session_id
    }

    pub fn version(&self) -> &'static str {
        &self.version
    }

    fn handshake(url: &Url) -> Result<Client<TcpStream>, DdpConnError> {
        // Handshake with the server
        Ok(ClientBuilder::new(&url.to_string())
            .map_err(|e| DdpConnError::Parse(e))?
            .connect_insecure().map_err(|e| DdpConnError::Network(e) )?)
    }

    fn negotiate(client: &mut Client<TcpStream>, version: &'static str) -> Result<NegotiateResp, DdpConnError> {
        let request = Connect::new(version);
        let request = Message::text(serde_json::to_string(&request).unwrap());

        try!( client.send_message(&request).map_err(|e| DdpConnError::Network(e)) );

        while let Ok(OwnedMessage::Text(plaintext)) = client.recv_message() {
            let decoded: Option<serde_json::Value> = serde_json::from_str(&plaintext).ok();
            if let Some(message) = decoded {
                if message.get("server_id").is_some() {
                    // DDP: Old API that will be deprecated and is not supported here.
                    continue;
                }
                match message.get("msg").and_then(|m| m.as_str()) {
                    Some("connected") => {
                        if let Some(session) = message.get("session").and_then(|v| v.as_str()) {
                            // TODO: Avoidable to_string?
                            return Ok(NegotiateResp::SessionId(session.to_string()));
                        }
                    },
                    Some("failed") => {
                        if let Some(version) = message.get("version").and_then(|v| v.as_str()) {
                            // TODO: Avoidable to_string?
                            return Ok(NegotiateResp::Version(version.to_string()));
                        }
                    }
                    _ => {
                        println!("{}", &plaintext);
                        break;
                    },
                }
            }
        }
        Err(DdpConnError::MalformedPacket)
    }

    fn connect(url: &Url) -> Result<(Client<TcpStream>, String, usize), DdpConnError> {
        let mut v_index = 0;

        loop {
            let mut client = try!( Connection::handshake(url) );
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

#[derive(Clone)]
struct Core {
    methods:    Arc<Mutex<Methods>>,
    mongos:     Arc<Mutex<HashMap<String, Arc<Collection>>>>,
    subs:       Arc<Mutex<Subscriptions>>,
    transfer:   Arc<Mutex<AtomicSender<String>>>,
}

impl Core {
    fn handle_ping(&self, message: &Value) {
        self.transfer.lock().unwrap().send(Pong::text(message.id())).unwrap();
    }

    fn handle_result(&self, message: &Value) {
        if let Some(ref id) = message.id() {
            let result = match (message.get("error"), message.get("result")) {
                (Some(e), None)    => Err(e),
                (None,    Some(r)) => Ok(r),
                _                  => return,
            };
            self.methods.lock().unwrap().apply(id, result);
        }
    }

    fn handle_added(&self, message: &Value) {
        let lock = self.mongos.lock().unwrap();
        let collection = self.collection(&lock, message);
        let id = message.id();

        if let (Some(id), Some(mongo)) = (id, collection) {
            let fields = message.fields();
            mongo.notify_insert(id, fields);
        }
    }

    fn handle_changed(&self, message: &Value) {
        let lock = self.mongos.lock().unwrap();
        let collection = self.collection(&lock, message);
        let id = message.id();

        if let (Some(id), Some(mongo)) = (id, collection) {
            let fields = message.fields();
            let cleared = message.cleared();
            mongo.notify_change(id, fields, cleared);
        }
    }

    fn handle_removed(&self, message: &Value) {
        let lock = self.mongos.lock().unwrap();
        let collection = self.collection(&lock, message);
        let id = message.id();

        if let (Some(id), Some(mongo)) = (id, collection) {
            mongo.notify_remove(id);
        }
    }

    fn handle_ready(&self, message: &Value) {
        let ids = message.subs().and_then(|s| s.as_array()).and_then(|a| {
            let idies: Vec<&str> = a.iter()
                .map(|id| id.as_str())
                .filter(|o| o.is_some())
                .map(|s| s.unwrap())
                .collect();
            Some(idies)
        });
        if let Some(ids) = ids {
            self.subs.lock().unwrap().notify(Ok(ids));
        }
    }

    fn handle_nosub(&self, message: &Value) {
        let id = message.id();
        let error = message.error();
        if let (Some(error), Some(id)) = (error, id) {
            self.subs.lock().unwrap().notify(Err((id, error)));
        }
    }

    #[inline]
    fn collection<'a>(&'a self, lock: &'a MongoLock, message: &Value) -> Option<&Arc<Collection>> {
        message.collection().and_then(|c| {
            lock.get(c)
        })
    }
}

struct Methods {
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

    fn send(&mut self, method: &str, params: Option<&Vec<&Ejson>>,
            callback: Box<FnMut(Result<&Ejson, &Ejson>) + Send + 'static>) {
        let id = self.rng.id();
        let method = Method::text(&id, method, params);
        self.outgoing.lock().unwrap().send(method).unwrap();
        self.pending_methods.insert(id, callback);
    }

    fn apply(&mut self, id: &str, response: Result<&Ejson, &Ejson>) {
        if let Some(method) = self.pending_methods.remove(id) {
            let mut method: MethodCallback = method;
            method(response);
        }
    }
}

pub struct Collection {
    remove_listeners: Arc<Mutex<HashMap<u32, Box<Fn(&str) + Send + 'static>>>>,
    insert_listeners: Arc<Mutex<HashMap<u32, Box<Fn(&str, Option<&Ejson>) + Send + 'static>>>>,
    change_listeners: Arc<Mutex<HashMap<u32, Box<Fn(&str, Option<&Ejson>, Option<&Ejson>) + Send + 'static>>>>,
    methods:          Arc<Mutex<Methods>>,
    subs:             Arc<Mutex<Subscriptions>>,
    id:               Arc<Mutex<Option<String>>>,
    count:            Arc<Mutex<u32>>,
    name:             String,
    ops:              OpNames,
}

impl Collection {
    fn new(name: String, core: &Core) -> Self {
        let ops = OpNames {
            insert: format!("/{}/insert", &name),
            update: format!("/{}/update", &name),
            upsert: format!("/{}/upsert", &name),
            remove: format!("/{}/remove", &name),
        };
        Collection {
            remove_listeners: Arc::new(Mutex::new(HashMap::new())),
            insert_listeners: Arc::new(Mutex::new(HashMap::new())),
            change_listeners: Arc::new(Mutex::new(HashMap::new())),
            methods:          core.methods.clone(),
            subs:             core.subs.clone(),
            id:               Arc::new(Mutex::new(None)),
            count:            Arc::new(Mutex::new(0)),
            name:             name,
            ops:              ops,
        }
    }

    fn notify_remove(&self, id: &str) {
        for listener in self.remove_listeners.lock().unwrap().values() {
            listener(id);
        }
    }

    fn notify_insert(&self, id: &str, fields: Option<&Ejson>) {
        for listener in self.insert_listeners.lock().unwrap().values() {
            listener(id, fields);
        }
    }

    fn notify_change(&self, id: &str, fields: Option<&Ejson>, cleared: Option<&Ejson>) {
        for listener in self.change_listeners.lock().unwrap().values() {
            listener(id, fields, cleared);
        }
    }

    fn increment(&self) -> u32 {
        let count = &mut *self.count.lock().unwrap();
        *count += 1;
        *count
    }

    pub fn on_remove<F>(&self, f: F) -> ListenerId
    where F: Fn(&str) + Send + 'static {
        let count = self.increment();
        self.remove_listeners.lock().unwrap().insert(count, Box::new(f));
        ListenerId(Listener::Removed, count)
    }

    pub fn on_add<F>(&self, f: F) -> ListenerId
    where F: Fn(&str, Option<&Ejson>) + Send + 'static {
        let count = self.increment();
        self.insert_listeners.lock().unwrap().insert(count, Box::new(f));
        ListenerId(Listener::Inserted, count)
    }

    pub fn on_change<F>(&self, f: F) -> ListenerId
    where F: Fn(&str, Option<&Ejson>, Option<&Ejson>) + Send + 'static {
        let count = self.increment();
        self.change_listeners.lock().unwrap().insert(count, Box::new(f));
        ListenerId(Listener::Changed, count)
    }

    pub fn on_ready<F>(&self, f: F)
    where F: FnMut(Result<(), &Ejson>) + Send + 'static {
        self.subs.lock().unwrap().add_listener(&mut *self.id.lock().unwrap(), f);
    }

    pub fn clear_listener(&self, id: ListenerId) {
        match id {
            ListenerId(Listener::Inserted, c) => { self.insert_listeners.lock().unwrap().remove(&c); },
            ListenerId(Listener::Changed,  c) => { self.change_listeners.lock().unwrap().remove(&c); },
            ListenerId(Listener::Removed,  c) => { self.remove_listeners.lock().unwrap().remove(&c); },
        }
    }

    pub fn insert<F>(&self, record: &Ejson, callback: F)
    where F: FnMut(Result<&Ejson, &Ejson>) + Send + 'static {
        self.methods.lock().unwrap().send(&self.ops.insert, Some(&vec![&record]), Box::new(callback));
    }

    pub fn update<F>(&self, selector: &Ejson, modifier: &Ejson, callback: F)
    where F: FnMut(Result<&Ejson, &Ejson>) + Send + 'static {
        self.methods.lock().unwrap().send(&self.ops.update, Some(&vec![&selector, &modifier]), Box::new(callback));
    }

    pub fn upsert<F>(&self, selector: &Ejson, modifier: &Ejson, callback: F)
    where F: FnMut(Result<&Ejson, &Ejson>) + Send + 'static {
        self.methods.lock().unwrap().send(&self.ops.upsert, Some(&vec![&selector, &modifier]), Box::new(callback));
    }

    pub fn remove<F>(&self, selector: &Ejson, callback: F)
    where F: FnMut(Result<&Ejson, &Ejson>) + Send + 'static {
        self.methods.lock().unwrap().send(&self.ops.remove, Some(&vec![&selector]), Box::new(callback));
    }

    pub fn subscribe(&self) {
        self.subs.lock().unwrap().sub(&self.name, &mut *self.id.lock().unwrap());
    }

    pub fn unsubscribe(&self) {
        let id_maybe = &mut *self.id.lock().unwrap();
        if let &mut Some(ref mut id) = id_maybe {
            self.subs.lock().unwrap().unsub(&id);
        }
        if id_maybe.is_some() {
            *id_maybe = None;
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

struct Subscriptions {
    outgoing: Arc<Mutex<AtomicSender<String>>>,
    subs:     HashMap<String, Vec<Box<FnMut(Result<(), &Ejson>) + Send + 'static>>>,
    rng:      Random,
}

impl Subscriptions {
    fn new(outgoing: Arc<Mutex<AtomicSender<String>>>) -> Self {
        Subscriptions {
            outgoing: outgoing,
            subs:     HashMap::new(),
            rng:      Random::new(),
        }
    }

    fn notify(&mut self, subs: Result<Vec<&str>, (&str, &Ejson)>) {
        match subs {
            Ok(successes) => {
                for id in successes.iter() {
                    self.relay(id, Ok(()));
                }
            },
            Err((id, err)) => self.relay(id, Err(err)),
        };
    }

    fn sub(&mut self, name: &str, id: &mut Option<String>) {
        if id.is_none() {
            self.create_profile(id);
        }
        if let &mut Some(ref id) = id {
            // TODO: Use the extra params.
            let sub_msg = Subscribe::text(&id, &name, None);
            self.outgoing.lock().unwrap().send(sub_msg).unwrap();
        }
    }

    fn unsub(&mut self, id: &str) {
        let unsub_msg = Unsubscribe::text(id);
        self.outgoing.lock().unwrap().send(unsub_msg).unwrap();
    }

    fn add_listener<F>(&mut self, id: &mut Option<String>, f: F)
    where F: FnMut(Result<(), &Ejson>) + Send + 'static {
        if id.is_none() {
            self.create_profile(id);
        }
        if let &mut Some(ref id) = id {
            if let Some(mut listeners) = self.subs.get_mut(id) {
                listeners.push(Box::new(f));
            }
        }
    }

    fn create_profile(&mut self, key: &mut Option<String>) {
        let id = self.rng.id();
        // Don't clone
        *key = Some(id.clone());
        self.subs.insert(id, Vec::new());
    }

    fn relay(&mut self, id: &str, data: Result<(), &Ejson>) {
        if let Some(mut callbacks) = self.subs.remove(id) {
            while let Some(mut callback) = callbacks.pop() {
                callback(data.clone());
            }
        }
    }
}

struct OnDrop(Arc<Fn() + Sync + Send>);

impl Drop for OnDrop {
    fn drop(&mut self) {
        self.0();
    }
}

impl OnDrop {
    fn consume(&self) {
    }
}

pub struct ConnectionHandle {
    sending:   JoinHandle<()>,
    receiving: JoinHandle<()>,
}

impl ConnectionHandle {
    pub fn join(self) {
        self.sending.join().ok();
        self.receiving.join().ok();
    }
}

pub struct ListenerId(Listener, u32);

enum Listener {
    Inserted,
    Removed,
    Changed,
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
    UrlIsNotWebsocket,
    IoError(io::Error),
    Parse(websocket::client::ParseError),
}

struct OpNames {
    insert: String,
    upsert: String,
    update: String,
    remove: String,
}

trait Reply<'a> {
    #[inline]
    fn id(&'a self) -> Option<&'a str> {
        self.get_ejson("id").and_then(|id| id.as_str())
    }

    #[inline]
    fn collection(&'a self) -> Option<&'a str> {
        self.get_ejson("collection").and_then(|c| c.as_str())
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

    fn get_ejson(&'a self, &str) -> Option<&'a Ejson>;
}

impl<'a> Reply<'a> for Value {
    #[inline]
    fn get_ejson(&'a self, key: &str) -> Option<&'a Ejson> {
        self.get(key)
    }
}

const WS:  &'static str = "ws";
const WSS: &'static str = "wss";
