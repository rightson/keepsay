// Tauri's build step (capabilities / ACL, Windows resource) only for Windows
// targets; elsewhere the crate is a stub (docs/changes/0016).

include!("src/command_list.rs");

fn main() {
    println!("cargo:rerun-if-changed=src/command_list.rs");
    if std::env::var_os("CARGO_CFG_WINDOWS").is_none() {
        return;
    }
    windows_build();
}

#[cfg(windows)]
fn windows_build() {
    let attrs = tauri_build::Attributes::new()
        .app_manifest(tauri_build::AppManifest::new().commands(COMMANDS));
    if let Err(e) = tauri_build::try_build(attrs) {
        panic!("tauri-build failed: {e:#}");
    }
}

#[cfg(not(windows))]
fn windows_build() {
    let _ = COMMANDS;
    panic!("the Windows app must be built on a Windows host");
}
