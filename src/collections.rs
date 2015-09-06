use std::borrow::Cow;
use std::sync::{Arc, Mutex};
use std::sync::mpsc::Sender as AtomicSender;
use std::collections::HashMap;

use Connection;
use Core;
use Methods;
use messages::{Ejson, Subscribe, Unsubscribe};
use random::Random;

pub struct ListenerId(Listener, u32);

enum Listener {
    Inserted,
    Removed,
    Changed,
}

pub struct MongoCallbacks<'s> {
    remove_listeners: Arc<Mutex<HashMap<u32, Box<Fn(&str) + Send + 'static>>>>,
    insert_listeners: Arc<Mutex<HashMap<u32, Box<Fn(&str, Option<&Ejson>) + Send + 'static>>>>,
    change_listeners: Arc<Mutex<HashMap<u32, Box<Fn(&str, Option<&Ejson>, Option<&Ejson>) + Send + 'static>>>>,
    methods:          Arc<Mutex<Methods>>,
    subs:             Arc<Mutex<Subscriptions>>,
    id:               Arc<Mutex<Option<String>>>,
    name:             Cow<'s, String>,
}

impl<'s> MongoCallbacks<'s> {
    pub fn new<F>(name: Cow<'s, String>, core: &Core) -> MongoCallbacks
    where F: Fn(&str) + 's {
        MongoCallbacks {
            remove_listeners: Arc::new(Mutex::new(HashMap::new())),
            insert_listeners: Arc::new(Mutex::new(HashMap::new())),
            change_listeners: Arc::new(Mutex::new(HashMap::new())),
            methods:          core.methods.clone(),
            subs:             core.subs.clone(),
            id:               Arc::new(Mutex::new(None)),
            name:             name,
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
}

pub trait Collection {
    fn on_remove<F>(&self, f: F) -> ListenerId
        where F: Fn(&str) + Send + 'static;

    fn on_add<F>(&self, f: F) -> ListenerId
        where F: Fn(&str, Option<&Ejson>) + Send + 'static;

    fn on_change<F>(&self, f: F) -> ListenerId
        where F: Fn(&str, Option<&Ejson>, Option<&Ejson>) + Send + 'static;

    fn on_ready<F>(&self, f: F)
        where F: FnMut(Result<(), &Ejson>) + Send + 'static;

    fn clear_listener(&self, id: ListenerId);

    fn insert<F>(&self, record: &Ejson, callback: F)
        where F: FnMut(Result<&Ejson, &Ejson>) + Send + 'static;

    fn update<F>(&self, selector: &Ejson, modifier: &Ejson, callback: F)
        where F: FnMut(Result<&Ejson, &Ejson>) + Send + 'static;

    fn upsert<F>(&self, selector: &Ejson, modifier: &Ejson, callback: F)
        where F: FnMut(Result<&Ejson, &Ejson>) + Send + 'static;

    fn remove<F>(&self, selector: &Ejson, callback: F)
        where F: FnMut(Result<&Ejson, &Ejson>) + Send + 'static;

    fn subscribe(&self);

    fn unsubscribe(&self);

    fn name(&self) -> &str;
}

impl Collection for MongoCallbacks {
    pub fn on_remove<F>(&self, f: F) -> ListenerId
    where F: Fn(&str) + Send + 'static {
        self.remove_listeners.lock().unwrap().insert(self.count, Box::new(f));
        ListenerId(Listener::Removed, self.count)
    }

    pub fn on_add<F>(&self, f: F) -> ListenerId
    where F: Fn(&str, Option<&Ejson>) + Send + 'static {
        self.insert_listeners.lock().unwrap().insert(self.count, Box::new(f));
        ListenerId(Listener::Inserted, self.count)
    }

    pub fn on_change<F>(&self, f: F) -> ListenerId
    where F: Fn(&str, Option<&Ejson>, Option<&Ejson>) + Send + 'static {
        self.change_listeners.lock().unwrap().insert(self.count, Box::new(f));
        ListenerId(Listener::Changed, self.count)
    }

    pub fn on_ready<F>(&self, f: F)
    where F: FnMut(Result<(), &Ejson>) + Send + 'static {
        self.subscription.lock().unwrap().add_listener(self.id.lock().unwrap(), f);
    }

    pub fn clear_listener(&self, id: ListenerId) {
        match id {
            ListenerId(Listener::Inserted, _) => { self.insert_listeners },
            ListenerId(Listener::Changed,  _) => { self.change_listeners },
            ListenerId(Listener::Removed,  _) => { self.remove_listeners },
        }.lock().unwrap().remove(&id.1);
    }

    pub fn insert<F>(&self, record: &Ejson, callback: F)
    where F: FnMut(Result<&Ejson, &Ejson>) + Send + 'static {
        // TODO: Stop this mess
        let method = format!("/{}/insert", self.name);
        self.methods.lock().unwrap().send(&method, Some(&vec![&record]), callback);
    }

    pub fn update<F>(&self, selector: &Ejson, modifier: &Ejson, callback: F)
    where F: FnMut(Result<&Ejson, &Ejson>) + Send + 'static {
        // TODO: Stop this mess
        let method = format!("/{}/update", self.name);
        self.methods.lock().unwrap().send(&method, Some(&vec![&selector, &modifier]), callback);
    }

    pub fn upsert<F>(&self, selector: &Ejson, modifier: &Ejson, callback: F)
    where F: FnMut(Result<&Ejson, &Ejson>) + Send + 'static {
        // TODO: Stop this mess
        let method = format!("/{}/upsert", self.name);
        self.methods.lock().unwrap().send(&method, Some(&vec![&selector, &modifier]), callback);
    }

    pub fn remove<F>(&self, selector: &Ejson, callback: F)
    where F: FnMut(Result<&Ejson, &Ejson>) + Send + 'static {
        // TODO: Stop this mess
        let method = format!("/{}/remove", self.name);
        self.methods.lock().unwrap().send(&method, Some(&vec![&selector]), callback);
    }

    pub fn subscribe(&self) {
        self.subs.lock().unwrap().sub(&self.name, self.id.lock().unwrap());
    }

    pub fn unsubscribe(&self) {
        if let Some(mut id) = self.id.lock().unwrap() {
            self.subs.lock().unwrap().unsub(&id);
            *id = None;
        }
    }

    pub fn name(&self) -> &str {
        &self.name
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

    pub fn sub(&mut self, name: &str, id: &mut Option<String>) {
        if id.is_none() {
            self.create_profile(id);
        }
        if let Some(id) = id {
            // TODO: Use the extra params.
            let sub_msg = Subscribe::text(&id, &name, None);
            self.outgoing.lock().unwrap().send(sub_msg).unwrap();
        }
    }

    pub fn unsub(&mut self, id: &str) {
        let unsub_msg = Unsubscribe::text(id);
        self.outgoing.lock().unwrap().send(unsub_msg).unwrap();
    }

    pub fn add_listener<F>(&mut self, id: &mut Option<String>, f: F)
    where F: FnMut(Result<(), &Ejson>) + Send + 'static {
        if id.is_none() {
            self.create_profile(id);
        }
        if let Some(ref id) = id {
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
