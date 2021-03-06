#[macro_use] extern crate chan;
extern crate docopt;
extern crate env_logger;
#[macro_use] extern crate lazy_static;
extern crate libclient;
#[macro_use] extern crate log;
extern crate lru_time_cache;
extern crate regex;
extern crate rustc_serialize;
extern crate strsim;
extern crate termbox_sys as termbox;
extern crate time;
extern crate toml;

mod store;
mod tui;
mod utils;

use docopt::Docopt;

use tui::{TUI, TUIError};
use utils::show_version_and_exit;

const DEFAULT_HOST: &'static str = "http://marietje-noord.marie-curie.nl/api";

const USAGE: &'static str = "
Usage:
  maruska [ --host=HOST ]
  maruska ( --help | --version )

Options:
  -H --host HOST        Hostname of marietje server
  -h --help             Display this message
  --version             Print version info and exit
";

#[derive(Debug, RustcDecodable)]
pub struct Args {
    flag_host: Option<String>,
    flag_help: bool,
    flag_version: bool,
}

fn main() {
    // initialize logger
    if let Err(err) = env_logger::init() {
        panic!("Failed to initialize logger: {}", err);
    }

    let args: Args = Docopt::new(USAGE)
        .map(|d| d.help(true))
        .and_then(|d| d.decode())
        .unwrap_or_else(|e| e.exit());

    if args.flag_version {
        show_version_and_exit();
    }

    let host = &args.flag_host.unwrap_or_else(|| String::from(DEFAULT_HOST));
    let (mut tui, event_receivers) = match TUI::new(host) {
        Ok((tui, event_receivers)) => (tui, event_receivers),
        Err(err) => panic!("initialization error: {}", err),
    };
    let (client_r, tui_r, tick_r) = event_receivers;

    let mut exit_err: Option<TUIError> = None;
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
                Err(err) => {
                    exit_err = Some(err);
                    break;
                }
            },
            tick_r.recv() => {},
        }
        tui.draw();
    }
    if let Some(err) = exit_err {
        panic!("{}", err);
    }
}
