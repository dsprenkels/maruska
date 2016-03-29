use std::collections::BTreeMap;
use std::sync::mpsc::{channel, Receiver, Sender};
use rustc_serialize::json::{Json, ToJson};

use comet::{CometChannel, CometError, serve as comet_serve};

struct Client {
    comet: CometChannel,
    send_message_tx: Sender<Json>,
    recv_message_rx: Receiver<Json>,
}

impl Client {
    fn new(url: &str) -> Client {
        let (send_message_tx, send_message_rx) = channel();
        let (recv_message_tx, recv_message_rx) = channel();
        Client {
            comet: CometChannel::new(&url, send_message_rx, recv_message_tx).unwrap(),
            send_message_tx: send_message_tx,
            recv_message_rx: recv_message_rx,
        }
    }

    fn handle_message(&mut self, msg_contents: &Json) -> Result<(), CometError> {
        let msgtype = try!(Some(msg_contents)
            .and_then(|x| x.as_array())
            .and_then(|x| x.get(0))
            .and_then(|x| x.as_object())
            .and_then(|x| x.get("type"))
            .and_then(|x| x.as_string())
            .ok_or_else(|| CometError::MalformedResponse(("found no msg type", msg_contents.clone())))
        );
        match &msgtype {
            &"welcome" => Ok(()),
            &"playing" => self.handle_playing(msg_contents),
            &"requests" => self.handle_requests(msg_contents),
            &"login_token" => self.handle_login_token(msg_contents),
            &_ => panic!("unhandled message type {}", msgtype)
        }
    }

    fn handle_playing(&mut self, msg_content: &Json) -> Result<(), CometError> {
        println!("now playing: {}", msg_content);
        Ok(())
    }

    fn handle_requests(&mut self, msg_content: &Json) -> Result<(), CometError> {
        println!("current requests: {}", msg_content);
        Ok(())
    }

    fn handle_login_token(&mut self, msg_content: &Json) -> Result<(), CometError> {
        println!("got login token: {}", msg_content);
        Ok(())
    }

    fn follow(&mut self) -> Result<(), CometError> {
        let mut b = BTreeMap::new();
        b.insert("type".to_string(), "follow".to_json());
        b.insert("which".to_string(), vec!("playing".to_string(), "requests".to_string()).to_json());
        self.send_message_tx.send(b.to_json()).map_err(|x| CometError::from(x))
    }

    fn request_login_token(&mut self) -> Result<(), CometError> {
        let mut b = BTreeMap::new();
        b.insert("type".to_string(), "request_login_token".to_json());
        self.send_message_tx.send(b.to_json()).map_err(|x| CometError::from(x))
    }
}


pub fn it_works() {
    // let mut client = Client::new("http://192.168.1.100/api");
    let mut client = Client::new("http://noordslet.science.ru.nl/api");
    client.follow().unwrap();
    client.request_login_token().unwrap();
    comet_serve(&client.comet).unwrap();
    loop {
        let message = client.recv_message_rx.recv().unwrap();

    }
}
