use docopt::Docopt;

use libclient::Client;

#[derive(Debug, RustcDecodable)]
pub struct Args;

const USAGE: &'static str = "
List the current request queue

Usage:
  maruska queue [options]

Options:
  -h --help     Display this message
";

pub fn main(argv: Vec<String>, global_args: super::Args) {
    let args: Args = Docopt::new(USAGE)
        .map(|d| d.help(true))
        .map(|d| d.argv(argv))
        .and_then(|d| d.decode())
        .unwrap_or_else(|e| e.exit());
    execute(args, global_args);
}

pub fn execute(_: Args, global_args: super::Args) {
    let (mut client, client_r) = Client::new(&global_args.flag_host).unwrap();
    client.follow(vec!(String::from("requests")));
    client.serve();

    while client.get_requests() == &None {
        let message = client_r.recv().unwrap();
        client.handle_message(&message).unwrap();
    }

    for request in client.get_requests().clone().unwrap() {
        let media = request.media;
        let requested_by = if let Some(x) = request.by {x} else { String::from("marietje") };
        println!("{}: {} - {}", requested_by, media.artist, media.title);
    }
}
