// Native Windows argv forwarding avoids cmd.exe's 8191-character ceiling.
// Policy remains in the shared Node guard, not in this transport adapter.
use std::process::{Command, ExitCode};

fn main() -> ExitCode {
    let result = Command::new(env!("CLEARRA_LAUNCHER_NODE"))
        .arg(env!("CLEARRA_LAUNCHER_GUARD"))
        .args(std::env::args_os().skip(1))
        .status();
    match result {
        Ok(status) => ExitCode::from(status.code().unwrap_or(1) as u8),
        Err(error) => {
            eprintln!("Clearra compiler guard launch failed: {error}");
            ExitCode::FAILURE
        }
    }
}
