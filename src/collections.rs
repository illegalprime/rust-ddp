use std::sync::{Arc, Mutex};
use std::sync::mpsc::Sender as AtomicSender;
use std::collections::hash_map::HashMap;

use DdpClient;
use Methods;
use messages::{Ejson, Subscribe, Unsubscribe};
use random::Random;

pub struct ListenerId(Listener, u32);

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
    subscription:     Arc<Mutex<Subscriptions>>,
    count:            u32,
    name:             String,
    id:               Option<String>,
}

impl MongoCallbacks {
    pub fn new(subscription: Arc<Mutex<Subscriptions>>, name: String) -> MongoCallbacks {
        MongoCallbacks {
            remove_listeners: HashMap::new(),
            insert_listeners: HashMap::new(),
            change_listeners: HashMap::new(),
            subscription:     subscription,
            count:            0,
            name:             name,
            id:               None,
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

    pub fn add_ready<F>(&mut self, f: F) where F: FnMut(Result<(), &Ejson>) + Send + 'static {
        if self.id.is_none() {
            self.sub();
        }
        if let Some(ref id) = self.id {
            self.subscription.lock().unwrap().add_listener(id, f);
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

    pub fn clear_listener(&mut self, id: ListenerId) {
        match id {
            ListenerId(Listener::Inserted, pos) => { self.insert_listeners.remove(&pos); },
            ListenerId(Listener::Changed,  pos) => { self.change_listeners.remove(&pos); },
            ListenerId(Listener::Removed,  pos) => { self.remove_listeners.remove(&pos); },
        };
        self.decrement();
    }

    fn sub(&mut self) {
        self.id = Some(self.subscription.lock().unwrap().sub(&self.name));
    }

    fn increment(&mut self) {
        self.count += 1;
        if self.count == 1 && self.id.is_none() {
            self.sub();
        }
    }

    fn decrement(&mut self) {
        self.count -= 1;
        if self.count == 0 && self.id.is_some() {
            if let Some(ref id) = self.id {
                self.subscription.lock().unwrap().unsub(id);
            }
        }
    }
}

pub struct Subscriptions {
    outgoing: Arc<Mutex<AtomicSender<String>>>,
    subs:     HashMap<String, Vec<Box<FnMut(Result<(), &Ejson>) + Send + 'static>>>,
    rng:      Random,
}

impl Subscriptions {
    pub fn new(outgoing: Arc<Mutex<AtomicSender<String>>>) -> Self {
        Subscriptions {
            outgoing: outgoing,
            subs:     HashMap::new(),
            rng:      Random::new(),
        }
    }

    pub fn notify(&mut self, subs: Result<Vec<&str>, (&str, &Ejson)>) {
        match subs {
            Ok(successes) => {
                for id in successes.iter() {
                    self.relay(id, Ok(()));
                }
            },
            Err((id, err)) => self.relay(id, Err(err)),
        };
    }

    pub fn sub(&mut self, name: &str) -> String {
        let id = self.rng.id();
        // TODO: Stop cloning:
        self.subs.insert(id.clone(), Vec::new());
        // TODO: Use the extra params.
        let sub_msg = Subscribe::text(&id, &name, None);
        DdpClient::send(sub_msg, &self.outgoing);
        id
    }

    pub fn unsub(&self, id: &str) {
        let unsub_msg = Unsubscribe::text(id);
        DdpClient::send(unsub_msg, &self.outgoing);
    }

    pub fn add_listener<F>(&mut self, id: &str, f: F) where F: FnMut(Result<(), &Ejson>) + Send + 'static {
        if let Some(mut listeners) = self.subs.get_mut(id) {
            listeners.push(Box::new(f));
        }
    }

    fn relay(&mut self, id: &str, data: Result<(), &Ejson>) {
        if let Some(mut callbacks) = self.subs.remove(id) {
            while let Some(mut callback) = callbacks.pop() {
                callback(data.clone());
            }
        }
    }
}

pub struct MongoCollection {
    name: String,
    methods:      Arc<Mutex<Methods>>,
    callbacks:    Arc<Mutex<MongoCallbacks>>,
}

impl MongoCollection {
    pub fn new(collection: String, methods: Arc<Mutex<Methods>>, callbacks: Arc<Mutex<MongoCallbacks>>) -> Self {
        MongoCollection {
            name:         collection,
            methods:      methods,
            callbacks:    callbacks,
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

    pub fn on_ready<F>(&self, f: F) where F: FnMut(Result<(), &Ejson>) + Send + 'static {
        self.callbacks.lock().unwrap().add_ready(f);
    }

    pub fn clear_listener(&self, id: ListenerId) {
        self.callbacks.lock().unwrap().clear_listener(id);
    }

    pub fn insert<F>(&self, record: &Ejson, callback: F) where F: FnMut(Result<&Ejson, &Ejson>) + Send + 'static {
        let method = format!("/{}/insert", self.name);
        self.methods.lock().unwrap().send(&method, Some(&vec![&record]), callback);
    }

    pub fn update<F>(&self, selector: &Ejson, modifier: &Ejson, callback: F) where F: FnMut(Result<&Ejson, &Ejson>) + Send + 'static {
        let method = format!("/{}/update", self.name);
        self.methods.lock().unwrap().send(&method, Some(&vec![&selector, &modifier]), callback);
    }

    pub fn upsert<F>(&self, selector: &Ejson, modifier: &Ejson, callback: F) where F: FnMut(Result<&Ejson, &Ejson>) + Send + 'static {
        let method = format!("/{}/upsert", self.name);
        self.methods.lock().unwrap().send(&method, Some(&vec![&selector, &modifier]), callback);
    }

    pub fn remove<F>(&self, selector: &Ejson, callback: F) where F: FnMut(Result<&Ejson, &Ejson>) + Send + 'static {
        let method = format!("/{}/remove", self.name);
        self.methods.lock().unwrap().send(&method, Some(&vec![&selector]), callback);
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}
