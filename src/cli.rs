extern crate rustc_serialize;
extern crate docopt;

mod utils;

use docopt::Docopt;

use utils::show_version_and_exit;

const USAGE: &'static str = "
Usage:
  maruska <command> [args...]
  maruska [options]

Options:
  -h --help             Display this message
  --version             Print version info and exit
  -v --verbose          Use verbose output
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

#[derive(Debug, RustcDecodable)]
struct Args {
  arg_command: String,
  flag_help: bool,
  flag_version: bool,
  flag_verbose: bool,
  flag_username: String,
  flag_password: String,
  flag_yes: bool,
}


pub fn main() {
    let args: Args = Docopt::new(USAGE)
        .and_then(|d| d.decode())
        .unwrap_or_else(|e| e.exit()
    );

    if args.flag_version {
        show_version_and_exit();
    }

}
