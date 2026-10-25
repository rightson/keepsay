//! `SendInput` typing (FR-5, PROPOSAL §7.3).
//!
//! Text goes in as VK_PACKET events (`KEYEVENTF_UNICODE`), which bypass the
//! keyboard layout and the IME, so CJK text arrives as-is with any IME mode.
//! Line breaks are sent as VK_RETURN.
//!
//! `SendInput` into a higher-integrity window is blocked by UIPI without an
//! error (documented behaviour); the user then sees nothing inserted, and
//! the text is still not on the clipboard.

use std::hint::black_box;
use std::thread::sleep;
use std::time::{Duration, Instant};

use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT,
    KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, SendInput, VIRTUAL_KEY, VK_CONTROL, VK_LWIN, VK_MENU,
    VK_RETURN, VK_RWIN, VK_SHIFT,
};
use zeroize::Zeroize;

use super::Win32Error;
use super::plan::{Stroke, plan_strokes};

/// Events per `SendInput` call. Small batches keep slow apps from dropping
/// input.
const BATCH: usize = 64;
const BATCH_PAUSE: Duration = Duration::from_millis(4);

/// How long to wait for the user to let go of the hotkey's modifiers.
const MODIFIER_WAIT: Duration = Duration::from_millis(1500);

pub fn send_text(text: &str) -> Result<(), Win32Error> {
    let mut strokes = plan_strokes(text);
    let result = send_strokes(&strokes);
    strokes.zeroize();
    result
}

fn send_strokes(strokes: &[Stroke]) -> Result<(), Win32Error> {
    wait_modifiers_released();
    let mut events: Vec<INPUT> = Vec::with_capacity(BATCH);
    let mut result = Ok(());
    for chunk in strokes.chunks(BATCH / 2) {
        events.clear();
        for s in chunk {
            let (vk, scan, flags) = match *s {
                Stroke::Unit(u) => (VIRTUAL_KEY(0), u, KEYEVENTF_UNICODE),
                Stroke::Enter => (VK_RETURN, 0, KEYBD_EVENT_FLAGS(0)),
            };
            events.push(key(vk, scan, flags));
            events.push(key(vk, scan, flags | KEYEVENTF_KEYUP));
        }
        if !send(&events) {
            result = Err(Win32Error::Failed);
            break;
        }
        sleep(BATCH_PAUSE);
    }
    clear(&mut events);
    result
}

/// Ctrl+V, for the clipboard path.
pub fn send_paste() -> Result<(), Win32Error> {
    use windows::Win32::UI::Input::KeyboardAndMouse::VK_V;
    wait_modifiers_released();
    let none = KEYBD_EVENT_FLAGS(0);
    let events = [
        key(VK_CONTROL, 0, none),
        key(VK_V, 0, none),
        key(VK_V, 0, KEYEVENTF_KEYUP),
        key(VK_CONTROL, 0, KEYEVENTF_KEYUP),
    ];
    if send(&events) {
        Ok(())
    } else {
        Err(Win32Error::Failed)
    }
}

fn key(vk: VIRTUAL_KEY, scan: u16, flags: KEYBD_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: scan,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

fn send(events: &[INPUT]) -> bool {
    if events.is_empty() {
        return true;
    }
    // SAFETY: `events` is a valid, initialized slice of keyboard INPUTs and
    // the size argument is the size of one INPUT, as SendInput requires.
    let sent = unsafe { SendInput(events, size_of::<INPUT>() as i32) };
    sent as usize == events.len()
}

/// Overwrite the event buffer (it holds the text's code units).
fn clear(events: &mut Vec<INPUT>) {
    for e in events.iter_mut() {
        *e = INPUT::default();
    }
    black_box(&events);
    events.clear();
}

/// The hotkey is Alt+Shift+…: typing while Alt is still down would trigger
/// menu accelerators in the target app. Wait (bounded) for the user to let
/// go; never inject key-ups for keys the user is holding.
fn wait_modifiers_released() {
    let start = Instant::now();
    while any_modifier_down() && start.elapsed() < MODIFIER_WAIT {
        sleep(Duration::from_millis(15));
    }
}

fn any_modifier_down() -> bool {
    [VK_MENU, VK_SHIFT, VK_CONTROL, VK_LWIN, VK_RWIN]
        .iter()
        .any(|vk| {
            // SAFETY: GetAsyncKeyState takes a virtual-key code by value and
            // has no memory-safety preconditions.
            let state = unsafe { GetAsyncKeyState(i32::from(vk.0)) };
            state < 0
        })
}
