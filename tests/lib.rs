#[macro_use]
extern crate serde_json;
extern crate ddp;

use ddp::{Connection, Url};

#[test]
fn test_connect_version() {
    let url = Url::parse("ws://127.0.0.1:3000/websocket").unwrap(); // Get the URL
    let ddp_client_result = Connection::new(&url, || {
        println!("Everything closed!");
    });

    let (client, handle) = match ddp_client_result {
        Ok(cnh) => cnh,
        Err(err)   => panic!("An error occured: {:?}", err),
    };

    println!("The session id is: {} with DDP v{}", client.session(), client.version());

    let login_message = json!({
        "user": {
            "username": "rc_bot",
        },
        "password": "supersecret"
    });
    // let json = json::encode(&login_message).unwrap();
    println!("\n\nCalling a real method!\n\n");
    client.call("login", Some(&vec![&login_message]), Box::new(|result| {
        println!("Ran method, login");
        match result {
            Ok(output) => println!("got a result: {}", output),
            Err(error) => println!("got an error: {}", error),
        }
    }));
    handle.join();
//
//    println!("\n\nCalling a fake method!\n\n");
//    client.call("not_a_method", None, |result| {
//        print!("Ran method, ");
//        match result {
//            Ok(output) => println!("got a result: {}", output),
//            Err(error) => println!("got an error: {}", error),
//        }
//    });
//
//    println!("\n\nSubscribing to MongoColl!\n\n");
//    let mongo = client.mongo("MongoColl");
//    mongo.on_add(|id, _| {
//        println!("Added record with id: {}", &id);
//    });
//
//    println!("\n\nInserting a record!\n\n");
//    let record = json::Json::from_str("{ \"first ever meteor data from rust\": true }").unwrap();
//    mongo.insert(&record, |result| {
//        match result {
//            Ok(_) => println!("First every successful insertion into Meteor through rust!"),
//            Err(_) =>  println!("Damn! Got an error."),
//        };
//    });
//
//    println!("\n\nRemoving records...\n\n");
//    mongo.remove(&record, |result| {
//        println!("removed records {:?}", result);
//    });
//
//    println!("Subscribing to non existent collection.");
//    let nomongo = client.mongo("SomethingElse");
//    nomongo.on_ready(|result| {
//        match result {
//            Ok(()) => unreachable!(),
//            Err(e) => println!("Got an error, this is expected: {}", e),
//        }
//    });
}
