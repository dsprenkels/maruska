use std::collections::BTreeMap;
use std::sync::mpsc::{channel, Receiver, Sender};

use rustc_serialize::json::{Json, ToJson};
use time;

use comet::{CometChannel, CometError, serve as comet_serve};
use media::Media;

struct Playing {
    end_time: time::Timespec,
    media: Media
}


struct Client {
    // The wrapped comet channel
    comet: CometChannel,

    // The Sender used to send messages to the remote server through the comet channel
    send_message_tx: Sender<Json>,

    // The Receiver used to receive messages from the remote server
    recv_message_rx: Receiver<Json>,

    // What is currently playing
    playing: Option<Playing>,

    // Some login token acquired from the remote server
    login_token: Option<String>,
}

impl Client {
    fn new(url: &str) -> Client {
        let (send_message_tx, send_message_rx) = channel();
        let (recv_message_tx, recv_message_rx) = channel();
        Client {
            comet: CometChannel::new(&url, send_message_rx, recv_message_tx).unwrap(),
            send_message_tx: send_message_tx,
            recv_message_rx: recv_message_rx,
            playing: None,
            login_token: None
        }
    }

    fn handle_message(&mut self, msg: &Json) -> Result<(), CometError> {
        let msg_type = try!(Some(msg)
            .and_then(|x| x.as_object())
            .and_then(|x| x.get("type"))
            .and_then(|x| x.as_string())
            .ok_or_else(|| CometError::MalformedResponse(("found no msg type", msg.clone())))
        );
        match &msg_type {
            &"welcome" => Ok(()),
            &"playing" => self.handle_playing(msg),
            &"requests" => self.handle_requests(msg),
            &"login_token" => self.handle_login_token(msg),
            &_ => panic!("unhandled message type {}", msg_type)
        }
    }

    fn handle_playing(&mut self, msg: &Json) -> Result<(), CometError> {
        println!("now playing: {}", msg);
        Ok(())
    }

    fn handle_requests(&mut self, msg: &Json) -> Result<(), CometError> {
        println!("current requests: {}", msg);
        Ok(())
    }

    fn handle_login_token(&mut self, msg: &Json) -> Result<(), CometError> {
        let login_token = try!(msg.as_object()
            .and_then(|x| x.get("login_token"))
            .and_then(|x| x.as_string())
            .ok_or_else(|| CometError::MalformedResponse(("found no login_token string", msg.clone())))
        );
        self.login_token = Some(login_token.to_string());
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
        client.handle_message(&message).unwrap();
    }
}
