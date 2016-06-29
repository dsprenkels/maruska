#[macro_use] extern crate chan;
extern crate hyper;
#[macro_use] extern crate log;
extern crate openssl;
extern crate rustc_serialize;
extern crate time;

mod comet;
pub mod media;

use std::collections::HashMap;
use std::error::Error;
use std::fmt;
use std::thread;

use rustc_serialize::json::{decode, Json, ToJson};

use comet::{CometChannel, CometError, serve as comet_serve};
use media::{Media, Playing, Request};


const MD5_HASH_LENGTH: usize = 32;

macro_rules! make_json_hashmap {
    ( $( $key:expr => $val:expr ),* ) => {{
        let mut b = HashMap::new();
        $(
            b.insert(String::from($key), $val.to_json());
        )*
        b
    }}
}

#[derive(Debug)]
pub enum Message {
    Welcome,
    Playing,
    Requests,
    LoginToken,
    Login,
    LoginError(String),
    QueryMediaResults,
}

#[derive(Debug)]
pub enum ClientError {
    Comet(CometError),
}

#[derive(Debug)]
pub enum RequestStatus {
    Ok, Deferred
}

impl fmt::Display for ClientError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "client error: ({})", self)
    }
}

impl From<CometError> for ClientError {
    fn from(err: CometError) -> Self {
        ClientError::Comet(err)
    }
}

impl Error for ClientError {
    fn description(&self) -> &str {
        let ClientError::Comet(ref err) = *self;
        err.description()
    }
}

#[derive(Clone, Debug)]
pub struct Client {
    // The wrapped comet channel
    channel: CometChannel,

    // The Sender used to send messages to the remote server through the comet channel
    send_message_s: chan::Sender<Json>,

    // What is currently playing
    playing: Option<Playing>,

    /// What the current requests are
    requests: Option<Vec<Request>>,

    /// Store the access key for the users login session, if we have retrieved it from
    /// the server.
    access_key: Option<String>,

    /// Some login token acquired from the remote server
    login_token: Option<String>,

    /// Are we currently logged in?
    logged_in: bool,

    /// Are we waiting for a login token?
    waiting_for_login_token: bool,

    /// Are we waiting for a login response?
    waiting_for_login: bool,

    /// This is Some((username, secret, using_access_key)) if the client should login,
    /// but does not have a login_token at this moment
    deferred_login: Option<(String, String, bool)>,

    /// The current search query results
    qm_results: Vec<Media>,

    /// The current query_media query, if present
    qm_query: Option<String>,

    /// And the amount of results we requested for this token, so that we will know if we have
    /// reached the end of the results list.
    qm_requested_count: Option<usize>,

    /// The current query_media token, so that we will know if the results match the last query.
    qm_token: usize,

    /// The amount of results we want to have for this query
    qm_results_count: usize,

    /// true if we have received all results for the current query
    qm_done: bool,

    /// Are we currently waiting for query results?
    qm_waiting_for_token: Option<usize>,

    /// This is a list of all messages that should be sent after we are logged in
    deferred_after_login: Vec<Json>,
}

impl Client {
    pub fn new(url: &str) -> Result<(Client, chan::Receiver<Json>), ClientError> {
        let (send_message_s, send_message_r) = chan::async();
        let (recv_message_s, recv_message_r) = chan::async();
        let comet_channel = match CometChannel::new(&url, send_message_r, recv_message_s) {
            Ok(comet_channel) => comet_channel,
            Err(err) => return Err(ClientError::from(err)),
        };
        Ok((Client {
            channel: comet_channel,
            send_message_s: send_message_s,
            playing: None,
            requests: None,
            access_key: None,
            login_token: None,
            logged_in: false,
            waiting_for_login_token: false,
            waiting_for_login: false,
            deferred_login: None,
            qm_results: Vec::new(),
            qm_query: None,
            qm_token: 0,
            qm_results_count: 0,
            qm_requested_count: None,
            qm_done: true,
            qm_waiting_for_token: None,
            deferred_after_login: Vec::new()
        }, recv_message_r))
    }

    pub fn get_playing(&self) -> &Option<Playing> {
        &self.playing
    }

    pub fn get_requests(&self) -> &Option<Vec<Request>> {
        &self.requests
    }

    pub fn get_qm_results(&self) -> (&Vec<Media>, &bool) {
        (&self.qm_results, &self.qm_done)
    }

    pub fn serve(&self) -> Vec<thread::JoinHandle<Result<(), CometError>>> {
        comet_serve(&self.channel)
    }

    fn send_message<T: ToJson>(&mut self, obj: &T) {
        self.send_message_s.send(obj.to_json())
    }

    fn send_message_after_login<T: ToJson>(&mut self, obj: &T) -> RequestStatus {
        if self.logged_in {
            self.send_message(obj);
            RequestStatus::Ok
        } else {
            self.deferred_after_login.push(obj.to_json());
            RequestStatus::Deferred
        }
    }

    pub fn handle_message(&mut self, msg: &Json) -> Result<Message, ClientError> {
        let fail = || CometError::MalformedResponse(("found no msg type", msg.clone()));
        let msg_type = try!(Some(msg)
            .and_then(|x| x.as_object())
            .and_then(|x| x.get("type"))
            .and_then(|x| x.as_string())
            .ok_or_else(&fail)
        );
        match msg_type {
            "welcome" => Ok(Message::Welcome),
            "playing" => self.handle_playing(msg),
            "requests" => self.handle_requests(msg),
            "login_token" => self.handle_login_token(msg),
            "logged_in" => self.handle_logged_in(msg),
            "error_login" => self.handle_login_error(msg),
            "query_media_results" => self.handle_query_media_results(msg),
            _ => {
                debug!("unhandled message type in message: {}", msg);
                panic!("unhandled message type {}", msg_type);
            },
        }
    }

    fn handle_playing(&mut self, msg: &Json) -> Result<Message, ClientError> {
        let fail = || CometError::MalformedResponse(("found no playing object", msg.clone()));
        let playing = try!(msg.as_object()
            .and_then(|x| x.get("playing"))
            .ok_or_else(&fail)
            .map(|x| decode(&format!("{}", x)))
        );
        self.playing = Some(playing.unwrap());
        debug!("currently playing: {:?}", self.playing);
        Ok(Message::Playing)
    }

    fn handle_requests(&mut self, msg: &Json) -> Result<Message, ClientError> {
        let fail = || CometError::MalformedResponse(("found no requests array", msg.clone()));
        let requests_array = try!(msg.as_object()
            .and_then(|x| x.get("requests"))
            .and_then(|x| x.as_array())
            .ok_or_else(&fail)
        );
        let mut requests = Vec::with_capacity(requests_array.len());
        for x in requests_array.iter() {
            requests.push(decode::<Request>(&format!("{}", x)).unwrap());
        }
        self.requests = Some(requests);
        debug!("current requests: {:?}", self.requests);
        Ok(Message::Requests)
    }

    fn handle_login_token(&mut self, msg: &Json) -> Result<Message, ClientError> {
        let fail = || CometError::MalformedResponse(("found no login_token string", msg.clone()));
        let login_token = try!(msg.as_object()
            .and_then(|x| x.get("login_token"))
            .and_then(|x| x.as_string())
            .ok_or_else(&fail)
        );
        self.login_token = Some(String::from(login_token));
        self.waiting_for_login_token = false;
        debug!("current login_token: {:?}", self.login_token);
        if let Some((ref username, ref secret, using_access_key)) = self.deferred_login.clone() {
            self.do_login_inner(username, secret, using_access_key);
        }
        Ok(Message::LoginToken)
    }

    fn handle_logged_in(&mut self, msg: &Json) -> Result<Message, ClientError> {
        self.waiting_for_login = false;
        self.logged_in = true;

        let fail = || CometError::MalformedResponse(("found no accessKey string", msg.clone()));
        self.access_key = Some(try!(msg.as_object()
            .and_then(|x| x.get("accessKey"))
            .and_then(|x| x.as_string())
            .ok_or_else(&fail))
            .to_owned()
        );

        let mut messages = Vec::with_capacity(self.deferred_after_login.len());
        messages.append(&mut self.deferred_after_login);
        for message in messages {
            self.send_message(&message);
        }
        self.deferred_after_login.clear();
        Ok(Message::Login)
    }

    fn handle_login_error(&mut self, msg: &Json) -> Result<Message, ClientError> {
        let fail = || CometError::MalformedResponse(("found no message string", msg.clone()));
        let error_msg = try!(msg.as_object()
                                .and_then(|x| x.get("message"))
                                .and_then(|x| x.as_string())
                                .ok_or_else(&fail));

        debug!("login error: {}", error_msg);
        Ok(Message::LoginError(error_msg.to_owned()))
    }

    fn handle_query_media_results(&mut self, msg: &Json) -> Result<Message, ClientError> {
        let fail = || CometError::MalformedResponse(("found no token string", msg.clone()));
        let token = try!(msg.as_object()
            .and_then(|x| x.get("token"))
            .and_then(|x| x.as_u64())
            .map(|x| x as usize)
            .ok_or_else(&fail)
        );
        if self.qm_waiting_for_token.map_or(false, |x| x == token) {
            self.qm_waiting_for_token = None;
        } else {
            // assert that this token is outdated
            assert!(self.qm_waiting_for_token.map_or(true, |x| token < x));
            return Ok(Message::QueryMediaResults);
        }

        let results_array = try!(msg.as_object()
            .and_then(|x| x.get("results"))
            .and_then(|x| x.as_array())
            .ok_or_else(&fail)
        );

        self.qm_results.reserve(results_array.len());
        for x in results_array {
            self.qm_results.push(decode::<Media>(&format!("{}", x)).unwrap());
        }

        if results_array.len() >= self.qm_requested_count.unwrap() {
            // response was saturated
            self.maybe_query_media();
        } else {
            self.qm_done = true;
        }

        self.maybe_query_media();
        Ok(Message::QueryMediaResults)
    }

    pub fn follow_all(&mut self) {
        self.follow(vec!("playing".to_string(), "requests".to_string()))
    }

    pub fn follow(&mut self, which: Vec<String>) {
        for x in &which[..] {
            assert!(x == "playing" || x == "requests");
        }
        let b = make_json_hashmap!(
            "type" => "follow",
            "which" => which
        );
        self.send_message_s.send(b.to_json())
    }

    pub fn request_login_token(&mut self) {
        let b = make_json_hashmap!("type" => "request_login_token");
        self.waiting_for_login_token = true;
        self.send_message(&b)
    }

    pub fn do_login(&mut self, username: &str, password_hash: &str) {
        self.do_login_inner(username, password_hash, false)
    }

    pub fn do_login_accesskey(&mut self, username: &str, access_key: &str) {
        self.do_login_inner(username, access_key, true)
    }

    pub fn do_login_inner(&mut self, username: &str, secret: &str, using_access_key: bool) {
        if let Some(ref login_token) = self.login_token {
            self.deferred_login = None;
            let b = make_json_hashmap!(
                "type" => if using_access_key {"login_accessKey"} else {"login"},
                "username" => username,
                "hash" => md5(&format!("{}{}", secret, login_token))
            );
            self.waiting_for_login = true;
            self.send_message_s.send(b.to_json())
        } else {
            self.deferred_login = Some((String::from(username), String::from(secret), using_access_key));
            if !self.waiting_for_login_token {
                self.request_login_token()
            }
        }
    }

    pub fn update_query(&mut self, new_query: Option<&str>, count: usize) {
        // At this point, we could be in any state (so no preconditions to be checked)
        match new_query {
            new_query if self.qm_query.as_ref().map(|x| x.as_str()) == new_query &&
                         count > self.qm_results_count => {
                self.qm_results_count = count;
                self.maybe_query_media();
            },
            new_query if self.qm_query.as_ref().map(|x| x.as_str()) == new_query => {},
            new_query => {
                self.qm_done = false;
                self.qm_query = new_query.map(|x| x.to_string());
                self.qm_requested_count = None;
                self.qm_results_count = count;
                self.qm_results.clear();
                self.qm_waiting_for_token = None;
                self.maybe_query_media();
            }
        }
    }

    fn maybe_query_media(&mut self) {
        match () {
            _ if self.qm_done => {},
            _ if self.qm_query.is_none() => {},
            _ if self.qm_requested_count.map_or(false, |x| x >= self.qm_results_count) => {},
            _ if self.qm_results.len() >= self.qm_results_count => {},
            _ if self.qm_waiting_for_token.is_some() => {},
            _ => self.query_media(),
        }

    }

    fn query_media(&mut self) {
        use std::cmp::min;
        assert!(self.qm_query.is_some());

        let skip = self.qm_results.len();
        self.qm_token += 1;

        // We don't want to make requests with more than `qm_chunk_size()` results,
        // because it would introduce too much lag. So if the user (interface)
        // requests more than `count` results, we do them in subsequent requests.
        self.qm_requested_count = Some(min(self.qm_results_count - skip, self.qm_chunk_size()));

        let b = make_json_hashmap!(
            "type" => "query_media",
            "query" => self.qm_query,
            "token" => self.qm_token,
            "skip" => skip,
            "count" => self.qm_requested_count
        );
        self.qm_waiting_for_token = Some(self.qm_token);
        self.send_message(&b)
    }

    fn qm_chunk_size(&self) -> usize {
        match self.qm_results.len() {
            x if x <= 50 => 25, // not too much lag
            x if x <= 100 => 50,
            x if x <= 200 => 100,
            x if x <= 500 => 1000 - x,
            _ => 1000
        }
    }

    pub fn do_request(&mut self, media: &Media) -> RequestStatus {
        self.do_request_from_key(&media.key)
    }

    pub fn do_request_from_key(&mut self, key: &str) -> RequestStatus {
        let b = make_json_hashmap!("type" => "request", "mediaKey" => key);
        self.send_message_after_login(&b)
    }
}

pub fn md5(p: &str) -> String {
    use openssl::crypto::hash::{hash, Type};
    use std::fmt::Write;
    let mut c = String::with_capacity(MD5_HASH_LENGTH);
    for byte in hash(Type::MD5, p.as_bytes()) {
        write!(c, "{:02x}", byte).unwrap();
    }
    c
}

#[cfg(test)]
mod tests {
    #[test]
    fn md5() {
        use super::md5;
        assert_eq!(md5(""), "d41d8cd98f00b204e9800998ecf8427e");
        assert_eq!(md5("a"), "0cc175b9c0f1b6a831c399e269772661");
        assert_eq!(md5("abc"), "900150983cd24fb0d6963f7d28e17f72");
        assert_eq!(md5("message digest"), "f96b697d7cb7938d525a2f31aaf161d0");
        assert_eq!(md5("abcdefghijklmnopqrstuvwxyz"), "c3fcd3d76192e4007dfb496cca67e13b");
        assert_eq!(md5("ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789"),
                   "d174ab98d277d9f5a5611c2c9f419d9f");
        assert_eq!(md5("12345678901234567890123456789012345678901234567890123456789012345678901234567890"),
                   "57edf4a22be3c955ac49da2e2107b67a");
    }
}
