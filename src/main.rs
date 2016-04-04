#[macro_use] extern crate chan;
extern crate env_logger;
extern crate libclient;
#[macro_use] extern crate log;
extern crate termbox_sys as termbox;
extern crate time;

mod tui;

use libclient::Client;
use std::time::Duration;
use tui::{TUI, Error as TUIError};

const URL: &'static str = "http://noordslet.science.ru.nl/api";

fn main() {
    // initialize logger
    if let Err(err) = env_logger::init() {
        panic!("Failed to initialize logger: {}", err);
    }

    // initialize client
    let (mut client, client_r) = Client::new(URL);
    client.follow_all();
    client.serve();

    // initialize user interface
    let mut tui = TUI::new();
    let tui_r = TUI::run();

    loop {
        let timeout = chan::after(Duration::from_secs(1));
        chan_select! {
            client_r.recv() -> message => {
                if let Err(_) = client.handle_message(&message.unwrap()) {break}
            },
            tui_r.recv() -> event => match tui.handle_event(&mut client, event.unwrap()) {
                Ok(_) => {},
                Err(TUIError::Quit) => break,
                Err(TUIError::Custom(s)) => {
                    drop(tui);
                    panic!("{}", s)
                }
            },
            timeout.recv() => {},
        }
        tui.draw(&client);
    }

}
