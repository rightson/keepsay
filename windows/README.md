# windows/

Tauri 2 tray app (M3). Nothing lives here yet. The plan is in `docs/specs/PROPOSAL.md` §7.3.

The rules that apply here:
- The key is entered in a native Win32 dialog that Rust opens. It never enters the WebView (V5).
- No `shell`, `fs`, `http` or `updater` plugins. `deny.toml` bans them.
- CSP is `default-src 'self'; connect-src ipc: http://ipc.localhost`. Devtools are off in release builds.
- No IPC command returns the key.
- The hotkey mechanism is undecided (V8).
