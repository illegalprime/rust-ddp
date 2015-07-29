use std::sync::{Arc, Mutex};
use std::sync::mpsc::Sender as AtomicSender;
use std::collections::hash_map::HashMap;

use DdpClient;
use Methods;
use messages::{Ejson, Subscribe, Unsubscribe};
use random::Random;

struct ListenerId(Listener, u32);

enum SubStatus {
    NotSubscribed,
    Subscribed(String),
    Err(Ejson),
}

enum Listener {
    Inserted,
    Removed,
    Changed,
}

// TODO: change name to &str
pub struct MongoCallbacks {
    remove_listeners: HashMap<u32, Box<Fn(&str) + Send + 'static>>,
    insert_listeners: HashMap<u32, Box<Fn(&str, Option<&Ejson>) + Send + 'static>>,
    change_listeners: HashMap<u32, Box<Fn(&str, Option<&Ejson>, Option<&Ejson>) + Send + 'static>>,
    subscribed_listeners: Vec<Box<FnMut(Result<&str, &Ejson>) + Send + 'static>>,
    ready_listeners:      Vec<Box<FnMut() + Send + 'static>>,
    outgoing: Arc<Mutex<AtomicSender<String>>>,
    status:   SubStatus,
    ready:    bool,
    count:    u32,
    name:     String,
    rng:      Random,
}

impl MongoCallbacks {
    pub fn new(outgoing: Arc<Mutex<AtomicSender<String>>>, name: String) -> MongoCallbacks {
        MongoCallbacks {
            remove_listeners: HashMap::new(),
            insert_listeners: HashMap::new(),
            change_listeners: HashMap::new(),
            subscribed_listeners: Vec::new(),
            ready_listeners:      Vec::new(),
            status: SubStatus::NotSubscribed,
            count:  0,
            ready:  false,
            name:   name,
            rng:    Random::new(),
            outgoing: outgoing,
        }
    }

    pub fn add_remove<F>(&mut self, f: F) -> ListenerId where F: Fn(&str) + Send + 'static {
        self.increment();
        self.remove_listeners.insert(self.count, Box::new(f));
        ListenerId(Listener::Removed, self.count)
    }

    pub fn add_insert<F>(&mut self, f: F) -> ListenerId where F: Fn(&str, Option<&Ejson>) + Send + 'static {
        self.increment();
        self.insert_listeners.insert(self.count, Box::new(f));
        ListenerId(Listener::Inserted, self.count)
    }

    pub fn add_change<F>(&mut self, f: F) -> ListenerId where F: Fn(&str, Option<&Ejson>, Option<&Ejson>) + Send + 'static {
        self.increment();
        self.change_listeners.insert(self.count, Box::new(f));
        ListenerId(Listener::Changed, self.count)
    }

    pub fn add_subscribe<F>(&mut self, f: F) where F: FnMut(Result<&str, &Ejson>) + Send + 'static {
        if !self.subscribed() {
            self.subscribed_listeners.push(Box::new(f));
        }
    }

    pub fn add_ready<F>(&mut self, f: F) where F: FnMut() + Send + 'static {
        if (!self.ready) {
            self.ready_listeners.push(Box::new(f));
        }
    }

    pub fn notify_remove(&self, id: &str) {
        for listener in self.remove_listeners.values() {
            listener(id);
        }
    }

    pub fn notify_insert(&self, id: &str, fields: Option<&Ejson>) {
        for listener in self.insert_listeners.values() {
            listener(id, fields);
        }
    }

    pub fn notify_change(&self, id: &str, fields: Option<&Ejson>, cleared: Option<&Ejson>) {
        for listener in self.change_listeners.values() {
            listener(id, fields, cleared);
        }
    }

    pub fn notify_sub(&mut self, status: Result<&str, &Ejson>) {
        self.status = match status {
            Ok(id) => SubStatus::Subscribed(id.to_string()),
            Err(e) => SubStatus::Err(e.clone()),
        };
        while !self.subscribed_listeners.is_empty() {
            if let Some(mut callback) = self.subscribed_listeners.pop() {
                callback(status.clone());
            }
        }
    }

    pub fn notify_ready(&mut self) {
        self.ready = true;
        while !self.ready_listeners.is_empty() {
            if let Some(mut callback) = self.ready_listeners.pop() {
                callback();
            }
        }
    }

    pub fn clear_listener(&mut self, id: ListenerId) {
        match id {
            ListenerId(Listener::Inserted, pos) => { self.insert_listeners.remove(&pos); },
            ListenerId(Listener::Changed,  pos) => { self.change_listeners.remove(&pos); },
            ListenerId(Listener::Removed,  pos) => { self.remove_listeners.remove(&pos); },
        };
        self.decrement();
    }

    pub fn size(&self) -> u32 {
        self.count
    }

    pub fn subscribed(&self) -> bool {
        match self.status {
            SubStatus::Subscribed(_) => true,
            _ => false,
        }
    }

    pub fn id(&self) -> Option<&String> {
        match self.status {
            SubStatus::Subscribed(ref id) => Some(id),
            _ => None,
        }
    }

    fn increment(&mut self) {
        self.count += 1;
        if self.count == 1 {
            self.sub();
        }
    }

    fn decrement(&mut self) {
        self.count -= 1;
        if self.count == 0 {
            self.unsub();
        }
    }

    fn sub(&mut self) {
        let id = self.rng.id();
        let subscribe = Subscribe::text(&id, &self.name, None);
        DdpClient::send(subscribe, &self.outgoing);
    }

    fn unsub(&self) {
        if let Some(ref id) = self.id() {
            let unsub = Unsubscribe::text(&id);
            DdpClient::send(unsub, &self.outgoing);
        }
    }
}

pub struct MongoCollection {
    name: String,
    methods:   Arc<Mutex<Methods>>,
    callbacks: Arc<Mutex<MongoCallbacks>>,
}

impl MongoCollection {
    pub fn new(collection: String, methods: Arc<Mutex<Methods>>, callbacks: Arc<Mutex<MongoCallbacks>>) -> MongoCollection {
        MongoCollection {
            name: collection,
            methods: methods,
            callbacks: callbacks,
        }
    }

    pub fn on_remove<F>(&self, f: F) -> ListenerId where F: Fn(&str) + Send + 'static {
        self.callbacks.lock().unwrap().add_remove(f)
    }

    pub fn on_add<F>(&self, f: F) -> ListenerId where F: Fn(&str, Option<&Ejson>) + Send + 'static {
        self.callbacks.lock().unwrap().add_insert(f)
    }

    pub fn on_change<F>(&self, f: F) -> ListenerId where F: Fn(&str, Option<&Ejson>, Option<&Ejson>) + Send + 'static {
        self.callbacks.lock().unwrap().add_change(f)
    }

    pub fn on_subscribe<F>(&self, f: F) where F: FnMut(Result<&str, &Ejson>) + Send + 'static {
        self.callbacks.lock().unwrap().add_subscribe(f);
    }

    pub fn clear_listener(&self, id: ListenerId) {
        self.callbacks.lock().unwrap().clear_listener(id);
    }

    pub fn insert<F>(&self, record: &Ejson, callback: F) where F: FnMut(Result<&Ejson, &Ejson>) + Send + 'static {
        let method = format!("/{}/insert", self.name);
        self.methods.lock().unwrap().send(&method, Some(vec![&record]), callback);
    }

    pub fn update<F>(&self, selector: &Ejson, modifier: &Ejson, callback: F) where F: FnMut(Result<&Ejson, &Ejson>) + Send + 'static {
        let method = format!("/{}/update", self.name);
        self.methods.lock().unwrap().send(&method, Some(vec![&selector, &modifier]), callback);
    }

    pub fn upsert<F>(&self, selector: &Ejson, modifier: &Ejson, callback: F) where F: FnMut(Result<&Ejson, &Ejson>) + Send + 'static {
        let method = format!("/{}/upsert", self.name);
        self.methods.lock().unwrap().send(&method, Some(vec![&selector, &modifier]), callback);
    }

    pub fn remove<F>(&self, selector: &Ejson, callback: F) where F: FnMut(Result<&Ejson, &Ejson>) + Send + 'static {
        let method = format!("/{}/remove", self.name);
        self.methods.lock().unwrap().send(&method, Some(vec![&selector]), callback);
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}
