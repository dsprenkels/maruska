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

use std::str::FromStr;

use docopt::Docopt;

use tui::{TUI, TUIError};
use utils::show_version_and_exit;

const USAGE: &'static str = "
Usage:
  maruska ( --host=HOST | --help | --version )

Options:
  -H --host HOST        Hostname of marietje server
  -h --help             Display this message
  --version             Print version info and exit
";

#[derive(Debug, RustcDecodable)]
pub struct Args {
    flag_host: String,
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

    let (mut tui, event_receivers) = match TUI::new(&args.flag_host) {
        Ok((tui, event_receivers)) => (tui, event_receivers),
        Err(err) => panic!("initialization error: {}", err),
    };
    let (client_r, tui_r, tick_r) = event_receivers;

    let mut exit_err: Option<TUIError> = None;
    let mut seconds_inactive = 0;
    loop {
        chan_select! {
            client_r.recv() -> message => {
                if let Err(err) = tui.handle_message_from_client(&message.unwrap()) {
                    drop(tui);
                    panic!("{}", err)
                }
            },
            tui_r.recv() -> event => match tui.handle_event(event.unwrap()) {
                Ok(()) => {
                    seconds_inactive = 0;
                },
                Err(TUIError::Quit) => break,
                Err(err) => {
                    exit_err = Some(err);
                    break;
                }
            },
            tick_r.recv() => {
                seconds_inactive += 1;
                if std::env::var("TMOUT").ok()
                                         .and_then(|x| usize::from_str(&x).ok())
                                         .map_or(false, |x| seconds_inactive >= x) {
                    break;
                }
            },
        }
        tui.draw();
    }
    if let Some(err) = exit_err {
        panic!("{}", err);
    }
}
