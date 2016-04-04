#[macro_use] extern crate chan;
extern crate env_logger;
extern crate libclient;
#[macro_use] extern crate log;
extern crate ncurses;

mod tui;

use libclient::Client;
use tui::TUI;

const URL: &'static str = "http://noordslet.science.ru.nl/api";

fn main() {
    // initialize logger
    if let Err(err) = env_logger::init() {
        panic!("Failed to initialize logger: {}", err);
    }

    // initialize locale (best guess)
    let locale_conf = ncurses::LcCategory::all;
    ncurses::setlocale(locale_conf, "");

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
            tui_r.recv() -> ch => tui.handle_input(ch.unwrap()),
        }
        tui.redraw(&client);
    }

}
