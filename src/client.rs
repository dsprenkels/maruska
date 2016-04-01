use std::collections::BTreeMap;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::fmt;

use rustc_serialize::json::{decode, Json, ToJson};

use comet::{CometChannel, CometError, serve as comet_serve};
use media::{Media, Playing, Request};


const MD5_HASH_LENGTH: usize = 32;


#[derive(Debug)]
pub enum ClientError {
    Comet(CometError)
}

impl fmt::Display for ClientError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "client error: ({})", self)
    }
}

impl From<CometError> for ClientError {
    fn from(err: CometError) -> ClientError {
        ClientError::Comet(err)
    }
}


pub struct Client {
    // The wrapped comet channel
    channel: CometChannel,

    // The Sender used to send messages to the remote server through the comet channel
    send_message_tx: Sender<Json>,

    // The Receiver used to receive messages from the remote server
    recv_message_rx: Receiver<Json>,

    // What is currently playing
    playing: Option<Playing>,

    // What the current requests are
    requests: Option<Vec<Request>>,

    // When we get an access key, call the following callbacks (if present)
    access_key_callback: Option<Box<Fn(&str) -> ()>>,

    // Some login token acquired from the remote server
    login_token: Option<String>,

    // Are we currently logged in?
    logged_in: bool,

    // Are we waiting for a login token?
    waiting_for_login_token: bool,

    // Are we waiting for a login response?
    waiting_for_login: bool,

    // This is Some((username, secret, using_access_key)) if the client should login, but does not have a
    // login_token at this moment
    deferred_login: Option<(String, String, bool)>,

    // The current search query results
    qm_results: Vec<Media>,

    // The current query_media query, if present
    qm_query: Option<String>,

    // The current query_media token, so that we will know if the results match the last query
    qm_token: u64,

    qm_results_count: usize,

    // Are we currently waiting for query results?
    waiting_for_qm_results: bool,
}

impl Client {
    fn new(url: &str) -> Client {
        let (send_message_tx, send_message_rx) = channel();
        let (recv_message_tx, recv_message_rx) = channel();
        Client {
            channel: CometChannel::new(&url, send_message_rx, recv_message_tx).unwrap(),
            send_message_tx: send_message_tx,
            recv_message_rx: recv_message_rx,
            playing: None,
            requests: None,
            access_key_callback: None,
            login_token: None,
            logged_in: false,
            waiting_for_login_token: false,
            waiting_for_login: false,
            deferred_login: None,
            qm_results: Vec::new(),
            qm_query: None,
            qm_token: 0,
            qm_results_count: 0,
            waiting_for_qm_results: false
        }
    }

    fn handle_message(&mut self, msg: &Json) -> Result<(), ClientError> {
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
            &"logged_in" => self.handle_logged_in(msg),
            &"query_media_results" => self.handle_query_media_results(msg),
            &_ => panic!("unhandled message type {}", msg_type)
        }
    }

    fn handle_playing(&mut self, msg: &Json) -> Result<(), ClientError> {
        let playing = try!(msg.as_object()
            .and_then(|x| x.get("playing"))
            .ok_or_else(|| CometError::MalformedResponse(("found no playing object", msg.clone())))
            .map(|x| decode(&format!("{}", x)))
        );
        self.playing = Some(playing.unwrap());
        debug!("currently playing: {:?}", self.playing);
        Ok(())
    }

    fn handle_requests(&mut self, msg: &Json) -> Result<(), ClientError> {
        let requests_array = try!(msg.as_object()
            .and_then(|x| x.get("requests"))
            .and_then(|x| x.as_array())
            .ok_or_else(|| CometError::MalformedResponse(("found no requests array", msg.clone())))
        );
        let mut requests = Vec::with_capacity(requests_array.len());
        for x in requests_array.iter() {
            requests.push(decode::<Request>(&format!("{}", x)).unwrap());
        }
        self.requests = Some(requests);
        debug!("current requests: {:?}", self.requests);
        Ok(())
    }

    fn handle_login_token(&mut self, msg: &Json) -> Result<(), ClientError> {
        let login_token = try!(msg.as_object()
            .and_then(|x| x.get("login_token"))
            .and_then(|x| x.as_string())
            .ok_or_else(|| CometError::MalformedResponse(("found no login_token string", msg.clone())))
        );
        self.login_token = Some(String::from(login_token));
        self.waiting_for_login_token = false;
        debug!("current login_token: {:?}", self.login_token);
        if let Some((ref username, ref secret, using_access_key)) = self.deferred_login.clone() {
            self.do_login_inner(username, secret, using_access_key)
        } else {
            Ok(())
        }
    }

    fn handle_logged_in(&mut self, msg: &Json) -> Result<(), ClientError> {
        let access_key = try!(msg.as_object()
            .and_then(|x| x.get("accessKey"))
            .and_then(|x| x.as_string())
            .ok_or_else(|| CometError::MalformedResponse(("found no accessKey string", msg.clone())))
        );
        self.waiting_for_login = false;
        self.logged_in = true;
        for callback in self.access_key_callback.iter() {
            callback(access_key);
        }
        Ok(())
    }

    fn handle_query_media_results(&mut self, msg: &Json) -> Result<(), ClientError> {
        let token = try!(msg.as_object()
            .and_then(|x| x.get("token"))
            .and_then(|x| x.as_u64())
            .ok_or_else(|| CometError::MalformedResponse(("found no token string", msg.clone())))
        );
        if token != self.qm_token {
            return Ok(());
        }
        self.waiting_for_qm_results = false;

        let results_array = try!(msg.as_object()
            .and_then(|x| x.get("results"))
            .and_then(|x| x.as_array())
            .ok_or_else(|| CometError::MalformedResponse(("found no token string", msg.clone())))
        );

        for x in results_array {
            self.qm_results.push(decode::<Media>(&format!("{}", x)).unwrap());
        }

        if self.qm_results_count > self.qm_results.len() {
            // we need to do another request
            return self.query_media_inner()
        }

        Ok(())
    }

    pub fn follow(&mut self) -> Result<(), ClientError> {
        let mut b = BTreeMap::new();
        b.insert("type".to_string(), "follow".to_json());
        b.insert("which".to_string(), vec!("playing".to_string(), "requests".to_string()).to_json());
        self.send_message_tx.send(b.to_json()).map_err(|x| ClientError::from(CometError::from(x)))
    }

    pub fn request_login_token(&mut self) -> Result<(), ClientError> {
        let mut b = BTreeMap::new();
        b.insert("type".to_string(), "request_login_token".to_json());
        self.waiting_for_login_token = true;
        self.send_message_tx.send(b.to_json()).map_err(|x| ClientError::from(CometError::from(x)))
    }

    fn do_login(&mut self, username: &str, password: &str) -> Result<(), ClientError> {
        self.do_login_inner(username, password, false)
    }

    fn do_login_accesskey(&mut self, username: &str, access_key: &str,
                          callback: Option<Box<Fn(&str) -> ()>>) -> Result<(), ClientError> {
        self.access_key_callback = callback;
        self.do_login_inner(username, access_key, true)
    }

    pub fn do_login_inner(&mut self, username: &str, secret: &str, using_access_key: bool) -> Result<(), ClientError> {
        if let Some(ref login_token) = self.login_token {
            self.deferred_login = None;
            let message_type = match using_access_key {
                true => "login_accessKey",
                false => "login"
            };
            let hash = match using_access_key {
                true => md5(&format!("{}{}", secret, login_token)),
                false => md5(&format!("{}{}", md5(secret), login_token))
            };
            let mut b = BTreeMap::new();
            b.insert(String::from("type"), message_type.to_json());
            b.insert(String::from("username"), username.to_json());
            b.insert(String::from("hash"), hash.to_json());
            self.waiting_for_login = true;
            self.send_message_tx.send(b.to_json()).map_err(|x| ClientError::from(CometError::from(x)))
        } else {
            self.deferred_login = Some((String::from(username), String::from(secret), using_access_key));
            match self.waiting_for_login_token {
                true => Ok(()), // do nothing
                false => self.request_login_token()
            }
        }
    }

    pub fn query_media(&mut self, query: &str, count: usize) -> Result<(), ClientError> {
        match self.qm_query {
            Some(ref x) if query != x => self.qm_results.clear(),
            _ => {}
        }

        self.qm_query = Some(String::from(query));
        self.qm_results_count = count;
        self.query_media_inner()
    }

    fn query_media_inner(&mut self) -> Result<(), ClientError> {
        use std::cmp::min;

        self.qm_token += 1;
        let skip = self.qm_results.len();

        // We don't want to make requests with more than 100 results, because it would
        // introduce too much lag. So if the user (interface) requests more than `count`
        // results, we do them in subsequent requests.
        let count = min(self.qm_results_count - skip, 100);

        let mut b = BTreeMap::new();
        b.insert(String::from("type"), "query_media".to_json());
        b.insert(String::from("query"), self.qm_query.to_json());
        b.insert(String::from("token"), self.qm_token.to_json());
        b.insert(String::from("skip"), skip.to_json());
        b.insert(String::from("count"), count.to_json());
        self.waiting_for_qm_results = true;
        self.send_message_tx.send(b.to_json()).map_err(|x| ClientError::from(CometError::from(x)))
    }
}

fn md5(p: &str) -> String {
    use openssl::crypto::hash::{hash, Type};
    use std::fmt::Write;
    let mut c = String::with_capacity(MD5_HASH_LENGTH);
    for byte in hash(Type::MD5, p.as_bytes()) {
        write!(c, "{:02x}", byte).unwrap();
    }
    c
}

pub fn it_works() {
    // let mut client = Client::new("http://192.168.1.100/api");
    let mut client = Client::new("http://noordslet.science.ru.nl/api");
    client.query_media("", 500);
    client.follow().unwrap();


    comet_serve(&client.channel).unwrap();
    loop {
        let message = client.recv_message_rx.recv().unwrap();
        client.handle_message(&message).unwrap();
    }
}
