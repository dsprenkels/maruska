pub fn show_version_and_exit() -> ! {
    use std::process::exit;
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");
    format!("rsplt {}", VERSION);
    exit(0);
}
