//! Reading the API key from the user (FR-1, US-M1-23 ③).
//!
//! The key comes from stdin only: a hidden prompt when stdin is a terminal,
//! otherwise the first line of piped input. Never argv, never environment
//! variables. The line buffer is zeroized as soon as the `SecretString`
//! exists (INV-2).
//!
//! Hiding input without `unsafe` or a new dependency: on Unix we run the
//! system `stty -echo` against the inherited terminal and restore `stty echo`
//! afterwards (also on error, via a drop guard). Elsewhere echo cannot be
//! turned off without FFI, so we warn and read visibly; piping the key in is
//! the alternative.

use std::io::{self, BufRead, IsTerminal, Write};

use secrecy::SecretString;
use secrecy::zeroize::Zeroize;

/// Restores terminal echo when dropped.
struct EchoGuard {
    active: bool,
}

impl EchoGuard {
    fn disable() -> Self {
        Self {
            active: set_echo(false),
        }
    }
}

impl Drop for EchoGuard {
    fn drop(&mut self) {
        if self.active {
            set_echo(true);
        }
    }
}

#[cfg(unix)]
fn set_echo(on: bool) -> bool {
    use std::process::{Command, Stdio};
    Command::new("stty")
        .arg(if on { "echo" } else { "-echo" })
        .stdin(Stdio::inherit())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

#[cfg(not(unix))]
fn set_echo(_on: bool) -> bool {
    false
}

/// Read one line from stdin as the key. Leading/trailing whitespace is
/// trimmed. Returns `None` on EOF or an empty line.
pub fn read_key() -> io::Result<Option<SecretString>> {
    let stdin = io::stdin();
    let tty = stdin.is_terminal();
    let mut line = String::new();
    {
        let _guard = if tty {
            let mut err = io::stderr();
            let guard = EchoGuard::disable();
            if guard.active {
                write!(err, "Anthropic API key (input hidden): ")?;
            } else {
                writeln!(
                    err,
                    "warning: cannot hide input on this terminal; the key will be visible as you type."
                )?;
                write!(err, "Anthropic API key: ")?;
            }
            err.flush()?;
            Some(guard)
        } else {
            None
        };
        let r = stdin.lock().read_line(&mut line);
        if tty {
            // The user's Enter was not echoed either.
            eprintln!();
        }
        if let Err(e) = r {
            line.zeroize();
            return Err(e);
        }
    }
    let key = {
        let trimmed = line.trim();
        (!trimmed.is_empty()).then(|| SecretString::from(trimmed))
    };
    line.zeroize();
    Ok(key)
}
