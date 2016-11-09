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
