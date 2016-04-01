use std::process::exit;

pub fn show_version_and_exit() -> ! {
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");
    println!("rsplt {}", VERSION);
    exit(0);
}
