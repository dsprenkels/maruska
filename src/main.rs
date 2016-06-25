#[macro_use] extern crate chan;
extern crate env_logger;
#[macro_use] extern crate lazy_static;
extern crate libclient;
#[macro_use] extern crate log;
extern crate lru_time_cache;
extern crate regex;
extern crate rustc_serialize;
extern crate termbox_sys as termbox;
extern crate time;
extern crate toml;

mod store;
mod tui;

use tui::{TUI, Error as TUIError};

const URL: &'static str = "http://10.1.2.3/api";
// const URL: &'static str = "http://noordslet.science.ru.nl/api";

fn main() {
    // initialize logger
    if let Err(err) = env_logger::init() {
        panic!("Failed to initialize logger: {}", err);
    }

    let (mut tui, event_receivers) = TUI::new(URL);
    let (client_r, tui_r, tick_r) = event_receivers;

    loop {
        chan_select! {
            client_r.recv() -> message => {
                if let Err(err) = tui.handle_message_from_client(&message.unwrap()) {
                    drop(tui);
                    panic!("{}", err)
                }
            },
            tui_r.recv() -> event => match tui.handle_event(event.unwrap()) {
                Ok(()) => {},
                Err(TUIError::Quit) => break,
            },
            tick_r.recv() => {},
        }
        tui.draw();
    }}
