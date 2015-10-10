use websocket::client::sender;
use websocket::client::receiver;
use websocket::ws::receiver::Receiver;
use websocket::ws::sender::Sender;
use websocket::stream::WebSocketStream;
use websocket::result::WebSocketError;
use websocket::Message;

trait Socket {
    fn send(&self, msg: Message) -> Result<(), WebSocketError>;
}

struct QueuedSocket {
    sender:   Mutex<Box<Sender<WebSocketStream>>>,
    receiver: receiver::Receiver<WebSocketStream>,
}

impl Socket for QueuedSocket {
    pub fn send(&self, msg: Message) -> Result<(), WebSocketError> {
        match sender.try_lock() {
            Ok(locked) => {
                match locked.send_message(msg) {
                    Err(_) => 
                }
            }
        }
    }

    fn try_send
}
