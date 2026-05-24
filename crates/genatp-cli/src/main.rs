//! genATP CLI entrypoint. All logic lives in `genatp_cli::run` so it stays
//! unit-testable; this file only wires `main` to the library.

#![forbid(unsafe_code)]

fn main() {
    std::process::exit(genatp_cli::run());
}
