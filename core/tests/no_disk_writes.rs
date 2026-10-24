//! INV-5: audio and transcripts never touch the disk (US-M1-25 ②).
//!
//! Text scan, like `downloader.rs`: the STT and polish paths must not use
//! any filesystem API. Comment lines are ignored. Covers the files that
//! exist at the time of writing; add new `stt/` or `polish/` files here.

const FILES: &[(&str, &str)] = &[
    ("stt/mod.rs", include_str!("../src/stt/mod.rs")),
    ("stt/whisper.rs", include_str!("../src/stt/whisper.rs")),
    ("polish/mod.rs", include_str!("../src/polish/mod.rs")),
    ("polish/guard.rs", include_str!("../src/polish/guard.rs")),
    ("polish/modes.rs", include_str!("../src/polish/modes.rs")),
    ("polish/prompt.rs", include_str!("../src/polish/prompt.rs")),
];

const FORBIDDEN: &[&str] = &[
    "std::fs",
    "fs::",
    "File::",
    "OpenOptions",
    "tempfile",
    "tempdir",
    "temp_dir",
];

#[test]
fn stt_and_polish_do_not_use_the_filesystem() {
    for (name, src) in FILES {
        let code: String = src
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        for forbidden in FORBIDDEN {
            assert!(
                !code.contains(forbidden),
                "{name} must not use {forbidden} (INV-5)"
            );
        }
    }
}
