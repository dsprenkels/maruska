extern crate docopt;
extern crate libclient;
#[macro_use] extern crate log;
extern crate rustc_serialize;
extern crate strsim;

mod playing;
mod queue;
mod utils;

use docopt::{Docopt, Error as DocoptError};
use strsim::levenshtein;
use utils::show_version_and_exit;

const USAGE: &'static str = "
Usage:
  maruska --host=HOST <command> [<args>...]
  maruska [options]

Options:
  -h --help             Display this message
  --version             Print version info and exit
  -v --verbose          Use verbose output
  -H --host HOST        Hostname of marietje server
  -u --username USER    Use a different username (than `whoami`)
  -p --password PASSWD  Provide a password on the command line
  -y --yes              Run non-interactively (assume yes)

Commands:
  playing      Get the currently playing song
  queue        List the current queue
  search       Search the songs list for a particular query
  request      Request playback one or more songs
  skip         Skip the currenly playing song (alias for `maruska remove 0`)
  remove       Cancel a song from the queue
  up           Move a song up in the queue
  down         Move a song down in the queue
  help         Get some help with another command
";

const COMMANDS: [&'static str; 9] = [
    "playing",
    "queue",
    "search",
    "request",
    "skip",
    "remove",
    "up",
    "down",
    "help",
];

#[derive(Debug, RustcDecodable)]
pub struct Args {
    arg_command: Option<String>,
    arg_args: Vec<String>,
    flag_help: bool,
    flag_version: bool,
    flag_verbose: bool,
    flag_host: String,
    flag_username: String,
    flag_password: String,
    flag_yes: bool,
}


pub fn main() {
    let args: Args = Docopt::new(USAGE)
        .map(|d| d.options_first(true))
        .map(|d| d.help(true))
        .and_then(|d| d.decode())
        .unwrap_or_else(|e| e.exit());

    if args.flag_version {
        show_version_and_exit();
    }

    match &args.arg_command.clone().unwrap()[..] {
        "playing" => {
            let argv = ["maruska", "playing"].into_iter()
                .map(|x| String::from(*x))
                .chain(args.arg_args.clone())
                .collect();
            playing::main(argv, args)
        },
        "queue" => {
            let argv = ["maruska", "queue"].into_iter()
                .map(|x| String::from(*x))
                .chain(args.arg_args.clone())
                .collect();
            queue::main(argv, args)
        }
        "search" => unimplemented!(),
        "request" => unimplemented!(),
        "skip" => unimplemented!(),
        "remove" => unimplemented!(),
        "up" => unimplemented!(),
        "down" => unimplemented!(),
        "help" => unimplemented!(),
        command => command_not_found(command)
    }
}

fn command_not_found(command: &str) -> ! {
    let mut other_command_dist: (Option<(&str, usize)>) = None;
    for x in COMMANDS.iter() {
        let dist = levenshtein(&command, x);
        match other_command_dist {
            None if dist <= 3 => {
                other_command_dist = Some((&x, dist));
            },
            Some((_, other_dist)) if dist < other_dist => {
                other_command_dist = Some((&x, dist));
            },
            _ => {}
        }
    }
    let msg = match other_command_dist {
        Some((other_command, _)) => format!("No such subcommand: '{}'. Did you mean '{}'?",
                                           command, other_command),
        None => format!("No such subcommand: '{}'", command)
    };
    let err = DocoptError::Argv(msg);
    let usage_str = USAGE
        .trim()
        .lines()
        .take(3)
        .collect::<Vec<&str>>()
        .join("\n");
    DocoptError::WithProgramUsage(Box::new(err), usage_str).exit();
}
