# maruska

`maruska` is a client for the [`marietje`](https://github.com/marietje/marietje)
music playing daemon. If you do not know what `marietje` is, you are probably
lost and you should go [somewhere else](https://www.musicpd.org/).

## Building `maruska`

Download and install the Rust compiler and Cargo from
[here](https://www.rust-lang.org). Then compile using Cargo:

```shell
# Compile `maruska`
cargo build --release   # omit `--release` for a debug build

# Execute the terminal UI
./target/release/maruska
```

## Comet channels

The (new) marietje server daemon does not use plain sockets anymore. Instead it
uses JSON messages over HTTP. This is quite safer and overcomes a lot of
encoding and serialization issues, because we can reuse safe and reliable JSON
encoders and decoders from the internet.

Back in the day, when the backend code was written, WebSockets didn't exist
yet. So for the persistent connections, we implemented the system using [long
polling requests](https://en.wikipedia.org/wiki/Push_technology#Long_polling).
While behind the scenes these long polling requests run, towards the interface
it is abstracted into a single "comet" channel. This channel allows for two-way
communication to the `maried` server.

If you plan to build your own front-end, look at [`comet.rs`
](https://github.com/dsprenkels/maruska/blob/master/src/libclient/comet.rs).
You can also send a pull request to `maried` to allow it to use WebSockets.
Then you can just use WebSockets, and in the meantime you'll have made the
world a slightly better place.

## Questions

Feel free to send me an email on my Github associated e-mail address.
