use docopt::Docopt;

use libclient::Client;

#[derive(Debug, RustcDecodable)]
pub struct Args;

const USAGE: &'static str = "
Retrieve the song that is currently played

Usage:
  maruska playing [options]

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
    let (mut client, client_r) = Client::new(&global_args.flag_host);
    client.follow(vec!(String::from("playing")));
    client.serve();

    while client.get_playing() == &None {
        let message = client_r.recv().unwrap();
        client.handle_message(&message).unwrap();
    }

    let playing = client.get_playing().clone().unwrap();
    let media = playing.media;
    if let Some(requested_by) = playing.requested_by {
        println!("{} - {} (requested by {})", media.artist, media.title, requested_by);
    } else {
        println!("{} - {} (requested at random by the server)", media.artist, media.title);
        };
}
