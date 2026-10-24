//! keepsay-cli: developer tool. M1 adds `keepsay-cli <file.wav> [--mode M] [--translate LANG]`.

use std::process::ExitCode;

fn main() -> ExitCode {
    let arg = std::env::args().nth(1);
    match arg.as_deref() {
        Some("--version") => {
            eprintln!("keepsay-cli {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        _ => {
            eprintln!("usage: keepsay-cli --version\n(wav -> transcript -> polish arrives in M1)");
            ExitCode::from(2)
        }
    }
}
