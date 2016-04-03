use std::char;
use std::thread;
use std::sync::{Arc, Mutex};

use chan;

use libclient::Client;
use rustbox;


pub struct TUI {
    rustbox: Arc<Mutex<rustbox::RustBox>>,
    results_invalidated: bool,
    query_invalidated: bool,
    query: String,
}

impl TUI {
    pub fn new() -> TUI {
        let rustbox = match rustbox::RustBox::init(Default::default()) {
            Result::Ok(v) => v,
            Result::Err(e) => panic!("{}", e),
        };
        TUI {
            rustbox: Arc::new(Mutex::new(rustbox)),
            results_invalidated: true,
            query_invalidated: true,
            query: String::new(),
        }
    }

    pub fn run(&mut self) -> chan::Receiver<rustbox::Event> {
        let (tx, rx) = chan::sync(0);
        let local_rustbox = self.rustbox.clone();
        thread::spawn(|| TUI::mainloop(local_rustbox, tx));
        rx
    }

    fn mainloop(rustbox: Arc<Mutex<rustbox::RustBox>>,
                event_sender: chan::Sender<rustbox::Event>) {
        loop {
            let event_option = {
                let local_rustbox = rustbox.lock().unwrap();
                local_rustbox.poll_event(false)
            };
            match event_option {
                Ok(event) => event_sender.send(event),
                Err(err) => panic!("{}", err)
            }
        }
    }

    pub fn handle_event(&mut self, event: rustbox::Event) {
        match event {
            rustbox::Event::KeyEventRaw(_, _, _) => panic!("event should not be KeyEventRaw(_)"),
            rustbox::Event::KeyEvent(key) => self.handle_key_event(key),
            rustbox::Event::ResizeEvent(width, height) => self.handle_resize_event(width, height),
            rustbox::Event::MouseEvent(_, _, _) => unimplemented!(),
            rustbox::Event::NoEvent => {}
        }
    }

    fn handle_key_event(&mut self, key: rustbox::keyboard::Key) {
        use rustbox::keyboard::Key::*;

        match key {
            Enter => self.handle_input_linefeed(),
            Backspace => self.handle_input_backspace(),
            Char(ch @ '/') | Char(ch @ ':') => self.handle_input_cmdtypechar(ch), // '/' and ':'
            Ctrl('u') => self.handle_input_nak(),
            Char(ch @ ' '...'~') => self.handle_input_alphanum(ch),
            ch => panic!("handling of keycode {:?} is not implemented", ch)
        }
    }

    fn handle_resize_event(&mut self, width: i32, height: i32) {
        unimplemented!();
    }

    fn handle_input_backspace(&mut self) {
        self.query.pop();
        self.invalidate_querywindow()
    }

    fn handle_input_linefeed(&mut self) {
        unimplemented!();
    }

    fn handle_input_nak(&mut self) {
        if self.query.len() > 1 {
            self.query.truncate(1);
        } else {
            self.query.clear();
        }
        self.invalidate_querywindow()
    }

    fn handle_input_cmdtypechar(&mut self, ch: char) {
        if !self.query.is_empty() { return; }

        if self.query.len() == 0 {
            self.query.push(ch);
        }
        self.invalidate_querywindow()
    }

    fn handle_input_alphanum(&mut self, input_ch: char) {
        let ch_option = char::from_u32(input_ch as u32);
        match ch_option {
            Some(ch) => {
                if self.query.is_empty() { self.query.push('/') };
                self.query.push(ch);
            },
            None => unreachable!()
        }
        self.invalidate_querywindow()
    }

    pub fn redraw(&mut self, client: &Client) {
        if self.results_invalidated {
            self.redraw_resultswindow(client)
        }
        if self.query_invalidated {
            self.redraw_querywindow(client)
        }
    }

    fn redraw_resultswindow(&mut self, client: &Client) {
        // first line shows currently playing song
        if let &Some(ref playing) = client.get_playing() {
            let requested_by: &str = match playing.requested_by {
                Some(ref by) => &by,
                None => "marietje"
            };
            // ncurses::mvwaddnstr(self.resultswindow, 0, 0, requested_by, requested_by.len() as i32);
        }
        // ncurses::wrefresh(self.resultswindow);
        self.results_invalidated = false;

    }

    fn redraw_querywindow(&mut self, client: &Client) {
        // draw query field
        // for i in 0..ncurses::COLS {
            // ncurses::mvwaddch(self.querywindow, 0, i, ' ' as ncurses::chtype);
        // }
        // ncurses::mvwaddnstr(self.querywindow, 0, 0, &self.query, self.query.len() as i32);
        // ncurses::wrefresh(self.querywindow);
        self.query_invalidated = false;
    }

    pub fn invalidate_resultswindow(&mut self) {
        self.results_invalidated = true;
    }

    pub fn invalidate_querywindow(&mut self) {
        self.query_invalidated = true;
    }
}
