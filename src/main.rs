#[macro_use] extern crate chan;
extern crate env_logger;
extern crate libclient;
#[macro_use] extern crate log;
extern crate termbox_sys as termbox;
extern crate time;

mod tui;

use libclient::Client;
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
        chan_select! {
            client_r.recv() -> message => {
                match client.handle_message(&message.unwrap()) {
                    Ok(_) => tui.invalidate_resultswindow(),
                    Err(_) => break
                }
            },
            tui_r.recv() -> event => match tui.handle_event(event.unwrap()) {
                Ok(_) => {},
                Err(TUIError::Quit) => break,
                Err(TUIError::Custom(s)) => {
                    drop(tui);
                    panic!("{}", s)
                }
            },
        }
        tui.draw(&client);
    }

}
