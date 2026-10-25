# windows/

Tauri 2 tray app (M3) over keepsay-core. Design and status: `docs/changes/0016-windows-app.md`, PROPOSAL §7.3.

- `src-tauri/`: the Rust app (`keepsay-windows`, binary `keepsay.exe`). It only builds for Windows; on other targets it is a stub so the workspace and the platform-independent tests still run.
- `ui/`: the settings page and the recording overlay, served from the binary only.

Rules that apply here:
- The key is entered in a native Win32 dialog that Rust opens (`src-tauri/src/win32/dialog.rs`). It never enters the WebView, and no IPC command returns it (`src-tauri/tests/ipc_surface.rs`).
- `unsafe` is allowed only in `src-tauri/src/win32/` (docs/changes/0018). Every block has a `// SAFETY:` comment.
- No `shell`, `fs`, `http`, `dialog` or `updater` plugins. CSP is `default-src 'self'; connect-src ipc: http://ipc.localhost`. Devtools are off in release builds.
- Hotkeys use RegisterHotKey (Alt+Shift+Z / Alt+Shift+X by default). No keyboard hook, no Raw Input.

Build (on Windows): `cargo build -p keepsay-windows --release`. Without whisper.cpp (no cmake / clang): `--no-default-features`. CI: `.github/workflows/windows-app.yml`.
