//! Clipboard fallback (FR-5, T6, US-M3-08).
//!
//! 1. Back up every memory-based clipboard format (bounded in size).
//! 2. Put the text on the clipboard as `CF_UNICODETEXT`, together with
//!    `ExcludeClipboardContentFromMonitorProcessing`,
//!    `CanIncludeInClipboardHistory` = 0 and `CanUploadToCloudClipboard` = 0,
//!    so clipboard history (Win+V), cloud clipboard and well-behaved
//!    monitors skip it.
//! 3. Send Ctrl+V and give the target app time to read it.
//! 4. If nobody else wrote to the clipboard meanwhile, overwrite our text
//!    in place and put the backup back (marked the same way, so the restore
//!    does not show up as a new history entry). If someone did, leave their
//!    content alone.
//!
//! The backup lives only in memory and is zeroized. Formats that are GDI
//! handles (bitmaps, metafiles, palettes) or private handles are not
//! copied; Windows synthesizes `CF_BITMAP` from the `CF_DIB` that is.

use std::thread::sleep;
use std::time::Duration;

use windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL, HWND};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, EnumClipboardFormats, GetClipboardData,
    GetClipboardSequenceNumber, OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
};
use windows::Win32::System::Memory::{
    GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DestroyWindow, HWND_MESSAGE, WINDOW_EX_STYLE, WINDOW_STYLE,
};
use windows::core::{PCWSTR, w};
use zeroize::{Zeroize, Zeroizing};

use super::Win32Error;
use super::input::send_paste;

/// `CF_UNICODETEXT` (WinUser.h). Defined here so the `Win32_System_Ole`
/// feature is not needed for one constant.
const CF_UNICODETEXT: u32 = 13;

/// Largest backup kept, all formats together. Formats that do not fit are
/// skipped (and lost); typical text, HTML and image copies are far smaller.
const BACKUP_MAX_BYTES: usize = 64 * 1024 * 1024;

/// Time the target app gets to read the clipboard after Ctrl+V.
const PASTE_SETTLE: Duration = Duration::from_millis(400);

const OPEN_TRIES: u32 = 20;
const OPEN_RETRY: Duration = Duration::from_millis(15);

struct Saved {
    format: u32,
    data: Zeroizing<Vec<u8>>,
}

/// A message-only window to own the clipboard: with a NULL owner,
/// `SetClipboardData` fails after `EmptyClipboard`.
struct Owner(HWND);

impl Owner {
    fn new() -> Result<Self, Win32Error> {
        // SAFETY: creates a message-only window of the system "STATIC"
        // class; all pointer arguments are static wide strings or None.
        let hwnd = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE(0),
                w!("STATIC"),
                PCWSTR::null(),
                WINDOW_STYLE(0),
                0,
                0,
                0,
                0,
                Some(HWND_MESSAGE),
                None,
                None,
                None,
            )
        }
        .map_err(|_| Win32Error::Failed)?;
        Ok(Owner(hwnd))
    }
}

impl Drop for Owner {
    fn drop(&mut self) {
        // SAFETY: the window was created by this thread in `Owner::new` and
        // is destroyed exactly once.
        let _ = unsafe { DestroyWindow(self.0) };
    }
}

/// The clipboard is open while this lives.
struct Open;

impl Open {
    fn new(owner: &Owner) -> Result<Self, Win32Error> {
        for _ in 0..OPEN_TRIES {
            // SAFETY: `owner.0` is a live window owned by this thread.
            if unsafe { OpenClipboard(Some(owner.0)) }.is_ok() {
                return Ok(Open);
            }
            // Another app has it open; it is usually released quickly.
            sleep(OPEN_RETRY);
        }
        Err(Win32Error::Failed)
    }
}

impl Drop for Open {
    fn drop(&mut self) {
        // SAFETY: the clipboard was opened by `Open::new` on this thread.
        let _ = unsafe { CloseClipboard() };
    }
}

pub fn paste(text: &str) -> Result<(), Win32Error> {
    let owner = Owner::new()?;
    let formats = Formats::register();

    let (backup, seq) = {
        let _open = Open::new(&owner)?;
        let backup = backup();
        // SAFETY: the clipboard is open (`_open`) and owned by `owner`.
        unsafe { EmptyClipboard() }.map_err(|_| Win32Error::Failed)?;
        let mut units: Zeroizing<Vec<u16>> = Zeroizing::new(text.encode_utf16().collect());
        units.push(0);
        let wrote = set_bytes(CF_UNICODETEXT, as_bytes(&units));
        units.zeroize();
        if wrote.is_err() {
            // Put the user's content back before giving up.
            for s in &backup {
                let _ = set_bytes(s.format, &s.data);
            }
            return Err(Win32Error::Failed);
        }
        formats.mark();
        // SAFETY: no preconditions.
        (backup, unsafe { GetClipboardSequenceNumber() })
    };

    let pasted = send_paste();
    sleep(PASTE_SETTLE);

    let _open = Open::new(&owner)?;
    // SAFETY: no preconditions.
    if unsafe { GetClipboardSequenceNumber() } == seq {
        scrub_text();
        // SAFETY: the clipboard is open (`_open`) and owned by `owner`.
        let _ = unsafe { EmptyClipboard() };
        for s in &backup {
            let _ = set_bytes(s.format, &s.data);
        }
        if !backup.is_empty() {
            formats.mark();
        }
    } else {
        tracing::info!("clipboard changed during paste, not restoring");
    }
    pasted
}

/// The three "please ignore this" formats (Windows 10 1809+; older systems
/// simply ignore the registered names).
struct Formats {
    exclude_monitor: u32,
    history: u32,
    cloud: u32,
}

impl Formats {
    fn register() -> Self {
        // SAFETY: each argument is a static NUL-terminated wide string.
        unsafe {
            Formats {
                exclude_monitor: RegisterClipboardFormatW(w!(
                    "ExcludeClipboardContentFromMonitorProcessing"
                )),
                history: RegisterClipboardFormatW(w!("CanIncludeInClipboardHistory")),
                cloud: RegisterClipboardFormatW(w!("CanUploadToCloudClipboard")),
            }
        }
    }

    /// Add the formats to the clipboard that is currently open. A DWORD 0
    /// means "no" for history and cloud; any data works for the monitor
    /// exclusion.
    fn mark(&self) {
        let zero = 0u32.to_ne_bytes();
        for f in [self.exclude_monitor, self.history, self.cloud] {
            if f != 0 {
                let _ = set_bytes(f, &zero);
            }
        }
    }
}

fn as_bytes(units: &[u16]) -> &[u8] {
    // SAFETY: u16 has no padding and any byte pattern is a valid u8; the
    // length covers exactly the slice's memory.
    unsafe { std::slice::from_raw_parts(units.as_ptr().cast::<u8>(), size_of_val(units)) }
}

/// GDI-handle and private-handle formats cannot be copied as bytes.
fn copyable(format: u32) -> bool {
    !matches!(
        format,
        2 | 3 | 9 | 14 // CF_BITMAP, CF_METAFILEPICT, CF_PALETTE, CF_ENHMETAFILE
            | 0x80 | 0x82 | 0x83 | 0x8E // CF_OWNERDISPLAY, CF_DSP{BITMAP,METAFILEPICT,ENHMETAFILE}
            | 0x200..=0x3FF // CF_PRIVATEFIRST..CF_GDIOBJLAST
    )
}

/// Copy every copyable format. The clipboard must be open.
fn backup() -> Vec<Saved> {
    let mut out = Vec::new();
    let mut budget = BACKUP_MAX_BYTES;
    let mut format = 0u32;
    loop {
        // SAFETY: the clipboard is open; EnumClipboardFormats only reads it.
        format = unsafe { EnumClipboardFormats(format) };
        if format == 0 {
            break;
        }
        if !copyable(format) {
            continue;
        }
        // SAFETY: the clipboard is open; the handle stays owned by the
        // clipboard and is only read below while the clipboard is open.
        let Ok(handle) = (unsafe { GetClipboardData(format) }) else {
            continue;
        };
        if let Some(data) = read_global(HGLOBAL(handle.0), &mut budget) {
            out.push(Saved { format, data });
        }
    }
    if budget == 0 {
        tracing::warn!("clipboard backup truncated");
    }
    out
}

fn read_global(h: HGLOBAL, budget: &mut usize) -> Option<Zeroizing<Vec<u8>>> {
    if h.is_invalid() {
        return None;
    }
    // SAFETY: `h` is a global memory handle owned by the open clipboard.
    let size = unsafe { GlobalSize(h) };
    if size == 0 || size > *budget {
        return None;
    }
    // SAFETY: as above; GlobalLock returns null on failure.
    let p = unsafe { GlobalLock(h) };
    if p.is_null() {
        return None;
    }
    let mut data = Zeroizing::new(vec![0u8; size]);
    // SAFETY: `p` points to `size` readable bytes (GlobalSize) while the
    // handle is locked; `data` has exactly `size` bytes; no overlap.
    unsafe {
        std::ptr::copy_nonoverlapping(p.cast::<u8>(), data.as_mut_ptr(), size);
        let _ = GlobalUnlock(h);
    }
    *budget -= size;
    Some(data)
}

/// Put `bytes` on the open clipboard under `format`.
fn set_bytes(format: u32, bytes: &[u8]) -> Result<(), Win32Error> {
    // SAFETY: allocates a movable global block; checked below.
    let h = unsafe { GlobalAlloc(GMEM_MOVEABLE, bytes.len().max(1)) }
        .map_err(|_| Win32Error::Failed)?;
    // SAFETY: `h` was just allocated and is not shared.
    let p = unsafe { GlobalLock(h) };
    if p.is_null() {
        // SAFETY: `h` is ours and not on the clipboard.
        let _ = unsafe { GlobalFree(Some(h)) };
        return Err(Win32Error::Failed);
    }
    // SAFETY: the block holds at least `bytes.len()` bytes; no overlap.
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), p.cast::<u8>(), bytes.len());
        let _ = GlobalUnlock(h);
    }
    // SAFETY: the clipboard is open and owned by our window; on success the
    // system owns `h`, on failure we still do and free it.
    match unsafe { SetClipboardData(format, Some(HANDLE(h.0))) } {
        Ok(_) => Ok(()),
        Err(_) => {
            // SAFETY: SetClipboardData failed, so `h` is still ours.
            let _ = unsafe { GlobalFree(Some(h)) };
            Err(Win32Error::Failed)
        }
    }
}

/// Overwrite our text in the clipboard's own memory before it is freed.
/// The clipboard must be open and still hold our data.
fn scrub_text() {
    // SAFETY: the clipboard is open; the handle belongs to it.
    let Ok(handle) = (unsafe { GetClipboardData(CF_UNICODETEXT) }) else {
        return;
    };
    let h = HGLOBAL(handle.0);
    if h.is_invalid() {
        return;
    }
    // SAFETY: global memory handle owned by the open clipboard, which still
    // holds the block we allocated in `set_bytes`.
    unsafe {
        let size = GlobalSize(h);
        let p = GlobalLock(h);
        if !p.is_null() {
            std::ptr::write_bytes(p.cast::<u8>(), 0, size);
            let _ = GlobalUnlock(h);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handle_formats_are_not_copied() {
        for f in [2, 3, 9, 14, 0x80, 0x82, 0x83, 0x8E, 0x200, 0x3FF] {
            assert!(!copyable(f), "{f:#x}");
        }
        for f in [1, 7, 8, 13, 15, 16, 17, 0x81, 0xC000] {
            assert!(copyable(f), "{f:#x}");
        }
    }

    #[test]
    fn bytes_view() {
        let u = [0x0041u16, 0];
        assert_eq!(as_bytes(&u).len(), 4);
    }
}
