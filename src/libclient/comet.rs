use std::error::Error;
use std::fmt;
use std::io::Error as IOError;
use std::sync::{Arc, Mutex, RwLock, TryLockError};
use std::sync::mpsc::{Receiver, Sender, SendError, RecvError, TryRecvError};

use hyper;
use hyper::error::Error as HyperError;
use rustc_serialize::json::{Json, ParserError as JsonError, ToJson};


#[derive(Debug)]
pub enum CometError {
    Send(SendError<Json>),
    Recv(RecvError),
    Hyper(HyperError),
    IO(IOError),
    Json(JsonError),
    MalformedResponse((&'static str, Json))
}

impl fmt::Display for CometError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "comet error: ({})", self)
    }
}

impl From<SendError<Json>> for CometError {
    fn from(err: SendError<Json>) -> CometError {
        CometError::Send(err)
    }
}

impl From<RecvError> for CometError {
    fn from(err: RecvError) -> CometError {
        CometError::Recv(err)
    }
}

impl From<HyperError> for CometError {
    fn from(err: HyperError) -> CometError {
        CometError::Hyper(err)
    }
}

impl From<IOError> for CometError {
    fn from(err: IOError) -> CometError {
        CometError::IO(err)
    }
}

impl From<JsonError> for CometError {
    fn from(err: JsonError) -> CometError {
        CometError::Json(err)
    }
}

impl Error for CometError {
    fn description(&self) -> &str {
        match self {
            &CometError::Send(ref err) => err.description(),
            &CometError::Recv(ref err) => err.description(),
            &CometError::Hyper(ref err) => err.description(),
            &CometError::IO(ref err) => err.description(),
            &CometError::Json(ref err) => err.description(),
            &CometError::MalformedResponse(_) => "malformed response",
        }
    }
}


#[derive(Clone)]
pub struct CometChannel {
    // hyper client instance
    client: Arc<hyper::Client>,

    // amount of current outstanding requests
    current_requests: Arc<Mutex<u8>>,

    // receive messages to send from the front-end
    send_message_rx: Arc<Mutex<Receiver<Json>>>,

    // where to send messages recieved from the other endpoint
    recv_message_tx: Sender<Json>,

    // comet session id
    session_id: Arc<RwLock<Option<String>>>,

    // reference to the url string slice
    url: Arc<String>,
}

impl CometChannel {
    pub fn new<T: ToString>(url: T,
                            send_message_rx: Receiver<Json>,
                            recv_message_tx: Sender<Json>) -> Result<CometChannel, CometError> {
        let mut comet = CometChannel {
            client: Arc::new(hyper::Client::new()),
            current_requests: Arc::new(Mutex::new(0)),
            send_message_rx: Arc::new(Mutex::new(send_message_rx)),
            recv_message_tx: recv_message_tx,
            session_id: Arc::new(RwLock::new(None)),
            url: Arc::new(url.to_string()),
        };
        try!(CometChannel::connect(&mut comet));
        Ok(comet)
    }

    fn send(&mut self, msg: Json) -> Result<(), CometError> {
        let mut res = try!(self.client.post(&*self.url)
                                      .body(&msg.to_string())
                                      .send());
        let decoded = try!(Json::from_reader(&mut res));
        trace!("received packet: {}", decoded);
        self.handle_receive_packet(decoded)
    }

    fn handle_receive_packet(&mut self, packet: Json) -> Result<(), CometError> {
        try!(self.save_session_id(&packet));
        let packet_contents = try!(packet.as_array()
            .and_then(|x| x.get(1))
            .and_then(|x| x.as_array())
            .ok_or_else(|| CometError::MalformedResponse(("found no msg content",
                                                          packet.clone())))
        );
        for message in packet_contents {
            self.recv_message_tx.send(message.clone()).unwrap();
        }
        Ok(())
    }

    fn save_session_id(&mut self, packet: &Json) -> Result<(), CometError> {
        let session_id = try!(packet.as_array()
            .and_then(|x| x.get(0))
            .and_then(|x| x.as_string())
            .ok_or_else(|| CometError::MalformedResponse(("found no session id",
                                                          packet.clone())))
        );
        let mut x = self.session_id.write().unwrap();
        *x = Some(String::from(session_id));
        Ok(())
    }

    fn send_packet<'a, I>(&mut self, packet_contents: I) -> Result<(), CometError>
            where I : IntoIterator, I::Item : ToJson {
        let mut packet = Vec::new();
        if let Some(ref id) = *self.session_id.read().unwrap() {
            packet.push(id.clone().to_json());
        }

        for message in packet_contents.into_iter() {
            packet.push(message.to_json());
        }

        let json = packet.to_json();
        trace!("sending packet: {}", json);
        self.send(json)
    }

    pub fn connect(&mut self) -> Result<(), CometError> {
        {
            let x = self.current_requests.lock().unwrap();
            assert_eq!(*x, 0); // something has already been sent
            assert_eq!(*self.session_id.read().unwrap(), None); // already connected
        }
        info!("Connecting to {}", self.url);
        self.send([(); 0].to_json())
    }

    pub fn poll(&mut self) -> Result<(), CometError> {
        let messages: Vec<()> = Vec::new();
        self.send_packet(messages)
    }

    pub fn handle_send_message(&mut self) -> Result<(), CometError> {
        let message_contents: Json = {
            let rx = self.send_message_rx.lock().unwrap();
            try!(rx.recv())
        };
        self.send_packet(Some(message_contents))
    }

    /// will return True if a message was sent, otherwise false
    pub fn try_handle_send_message(&mut self) -> Result<bool, CometError> {
        let packet_contents: Vec<Json> = {
            let rx = match self.send_message_rx.try_lock() {
                Ok(rx) => rx,
                Err(TryLockError::WouldBlock) => return Ok(false),
                Err(TryLockError::Poisoned(err)) => panic!("{}", err),
            };

            let mut packet_contents = Vec::new();
            loop {
                let x = rx.try_recv();
                match x {
                    Ok(message) => packet_contents.push(message),
                    Err(TryRecvError::Empty) => break,
                    Err(err) => panic!("{}", err)
                }
            }

            if packet_contents.is_empty() {
                return Ok(false);
            }
            packet_contents
        };
        self.send_packet(packet_contents).map(|_| true)
    }
}

pub fn serve(shared_comet: &CometChannel) {
    use std::thread;

    if *shared_comet.session_id.read().unwrap() == None {
        panic!("I cannot serve when I'm not connected!")
    }

    for _ in 0..2 {
        let mut local_comet = shared_comet.clone();
        thread::spawn(move || -> Result<(), CometError> {
            loop {
                if try!(local_comet.try_handle_send_message()) {
                    continue
                } else {
                    // do we need to send a long poll request?
                    if {
                        let current_requests = local_comet.current_requests.clone();
                        let mut x = current_requests.lock().unwrap();
                        match *x {
                            0 => { *x += 1; true },
                            1 => false,
                            _ => unreachable!()
                        }
                    } {
                        try!(local_comet.poll());
                        let current_requests = local_comet.current_requests.clone();
                        let mut x = current_requests.lock().unwrap();
                        *x -= 1;
                    } else {
                        try!(local_comet.handle_send_message());
                    }
                }
            }
        });
    }
}
