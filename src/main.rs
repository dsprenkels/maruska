#[macro_use] extern crate chan;
extern crate env_logger;
extern crate libclient;
#[macro_use] extern crate log;
extern crate rustbox;

mod tui;

use libclient::{Client, MessageType};
use tui::TUI;

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
    let tui_r = tui.run();

    loop {
        chan_select! {
            client_r.recv() -> message => {
                match client.handle_message(&message.unwrap()) {
                    Ok(MessageType::Requests) => tui.invalidate_resultswindow(),
                    Ok(MessageType::Playing) => tui.invalidate_resultswindow(),
                    Ok(_) => {},
                    Err(err) => panic!("error: {}", err),
                }
            },
            tui_r.recv() -> event => tui.handle_event(event.unwrap()),
        }
        tui.redraw(&client);
    }

}
