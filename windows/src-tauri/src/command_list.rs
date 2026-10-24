// Every IPC command the app defines. Shared by build.rs (tauri-build
// generates an `allow-<command>` permission for each, and with an app
// manifest any command not granted by a capability is denied) and by the
// IPC surface test (tests/ipc_surface.rs), which checks this list against
// the `#[tauri::command]` handlers, `generate_handler!` and the
// capabilities. Adding a command means updating all four.
pub const COMMANDS: &[&str] = &[
    "get_settings",
    "save_settings",
    "key_status",
    "enter_key",
    "delete_key",
    "budget_status",
    "list_models",
    "download_model",
    "verify_model",
    "model_progress",
    "hotkey_status",
    "open_mic_settings",
    "overlay_state",
];
