//! INV-5: audio and transcripts never touch the disk (US-M1-25 ②).
//!
//! Text scan, like `downloader.rs`: the STT and polish paths must not use
//! any filesystem API. Comment lines are ignored. Every file in `stt/` and
//! `polish/` is scanned.

/// Every `.rs` file under these directories is scanned, so new files are
/// covered without editing this test.
const DIRS: &[&str] = &["src/stt", "src/polish"];

fn sources() -> Vec<(String, String)> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut out = Vec::new();
    for d in DIRS {
        for e in std::fs::read_dir(root.join(d)).expect("source dir") {
            let p = e.expect("entry").path();
            if p.extension().is_some_and(|x| x == "rs") {
                let name = p
                    .strip_prefix(root)
                    .expect("under root")
                    .display()
                    .to_string();
                out.push((name, std::fs::read_to_string(&p).expect("read")));
            }
        }
    }
    out
}

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
    let files = sources();
    // stt/{mod,whisper}.rs, polish/{mod,guard,modes,prompt,tests}.rs
    assert!(files.len() >= 7, "scan found {} files", files.len());
    for (name, src) in &files {
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
