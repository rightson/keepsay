//! Native API key dialog (V5, US-M3-03, docs/mockups/desktop.html "w32").
//!
//! A small modal window built with `CreateWindowExW` on the calling thread,
//! with its own message loop. One `ES_PASSWORD` edit (masked; the control
//! refuses copy / cut, and Windows refuses cross-process `WM_GETTEXT` on
//! password edits), OK and Cancel. The window is excluded from screen
//! capture (`WDA_EXCLUDEFROMCAPTURE`, falling back to `WDA_MONITOR` on
//! systems older than Windows 10 2004).
//!
//! On OK the text is read with `GetWindowTextW` into a zeroized UTF-16
//! buffer, trimmed in place, decoded into a `String` of exactly the right
//! capacity (so `SecretString::from` does not reallocate and leave a copy
//! behind), and the edit's text is overwritten before the window is
//! destroyed. The key never touches the WebView, the clipboard, a file or
//! a log (INV-1, INV-2).

use std::cell::RefCell;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

use secrecy::SecretString;
use windows::Win32::Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    COLOR_WINDOW, COLOR_WINDOWTEXT, CreateFontIndirectW, DeleteObject, GetDC, GetDeviceCaps,
    GetSysColor, GetSysColorBrush, HDC, HFONT, HGDIOBJ, LOGPIXELSY, ReleaseDC, SetBkColor,
    SetTextColor,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::EM_SETLIMITTEXT;
use windows::Win32::UI::Input::KeyboardAndMouse::{EnableWindow, SetFocus};
use windows::Win32::UI::WindowsAndMessaging::{
    AdjustWindowRectEx, BS_DEFPUSHBUTTON, BS_PUSHBUTTON, CreateWindowExW, DC_HASDEFID, DM_GETDEFID,
    DefWindowProcW, DestroyWindow, DispatchMessageW, EN_CHANGE, ES_AUTOHSCROLL, ES_PASSWORD,
    GetMessageW, GetWindowTextLengthW, GetWindowTextW, HMENU, IDC_ARROW, IsDialogMessageW,
    LoadCursorW, MSG, NONCLIENTMETRICSW, RegisterClassExW, SPI_GETNONCLIENTMETRICS,
    SPI_GETWORKAREA, SW_SHOW, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, SendMessageW,
    SetForegroundWindow, SetWindowDisplayAffinity, SetWindowTextW, ShowWindow,
    SystemParametersInfoW, TranslateMessage, WDA_EXCLUDEFROMCAPTURE, WDA_MONITOR, WINDOW_EX_STYLE,
    WINDOW_STYLE, WM_CLOSE, WM_COMMAND, WM_CTLCOLORSTATIC, WM_SETFONT, WNDCLASSEXW, WS_CAPTION,
    WS_CHILD, WS_DISABLED, WS_EX_CLIENTEDGE, WS_EX_CONTROLPARENT, WS_EX_DLGMODALFRAME,
    WS_EX_TOPMOST, WS_POPUP, WS_SYSMENU, WS_TABSTOP, WS_VISIBLE,
};
use windows::core::{PCWSTR, w};
use zeroize::Zeroizing;

use super::Win32Error;
use super::plan::trim_ascii_ws;

const CLASS: PCWSTR = w!("keepsay.KeyDialog");

const ID_OK: i32 = 1; // IDOK
const ID_CANCEL: i32 = 2; // IDCANCEL
const ID_EDIT: i32 = 100;

/// OK stays disabled below this many characters (core's format check is
/// the real gate: `sk-ant-` prefix, 20–512 bytes).
const MIN_CHARS: i32 = 8;
/// Matches core's maximum key length.
const MAX_CHARS: usize = 512;

/// Client area layout in 96-DPI pixels.
const CLIENT_W: i32 = 440;
const CLIENT_H: i32 = 176;
const MARGIN: i32 = 16;
const BTN_W: i32 = 88;
const BTN_H: i32 = 28;

/// Only one dialog at a time (a second "輸入金鑰…" click returns `Busy`).
static OPEN: AtomicBool = AtomicBool::new(false);

struct State {
    edit: HWND,
    ok: HWND,
    result: Option<SecretString>,
    done: bool,
}

thread_local! {
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
}

struct OpenFlag;

impl Drop for OpenFlag {
    fn drop(&mut self) {
        OPEN.store(false, Ordering::SeqCst);
    }
}

struct Font(HFONT);

impl Drop for Font {
    fn drop(&mut self) {
        if !self.0.is_invalid() {
            // SAFETY: the font was created by CreateFontIndirectW and every
            // window using it has been destroyed by now.
            let _ = unsafe { DeleteObject(HGDIOBJ(self.0.0)) };
        }
    }
}

pub fn prompt() -> Result<Option<SecretString>, Win32Error> {
    if OPEN.swap(true, Ordering::SeqCst) {
        return Err(Win32Error::Busy);
    }
    let _flag = OpenFlag;
    let hinst = instance()?;
    register_class(hinst)?;

    let dpi = system_dpi();
    let px = |v: i32| v * dpi / 96;
    let font = Font(message_font());

    let ex = WS_EX_DLGMODALFRAME | WS_EX_TOPMOST | WS_EX_CONTROLPARENT;
    let style = WS_POPUP | WS_CAPTION | WS_SYSMENU;
    let mut r = RECT {
        left: 0,
        top: 0,
        right: px(CLIENT_W),
        bottom: px(CLIENT_H),
    };
    // SAFETY: `r` is a valid RECT for the duration of the call.
    let _ = unsafe { AdjustWindowRectEx(&mut r, style, false, ex) };
    let (w, h) = (r.right - r.left, r.bottom - r.top);
    let (x, y) = centered(w, h);

    // SAFETY: the class was registered above for this module instance; the
    // strings are static and NUL-terminated.
    let hwnd = unsafe {
        CreateWindowExW(
            ex,
            CLASS,
            w!("keepsay — 輸入 API 金鑰"),
            style,
            x,
            y,
            w,
            h,
            None,
            None,
            Some(hinst),
            None,
        )
    }
    .map_err(|_| Win32Error::Failed)?;

    let child = |class: PCWSTR,
                 text: PCWSTR,
                 style: WINDOW_STYLE,
                 ex: WINDOW_EX_STYLE,
                 id: i32,
                 (cx, cy, cw, ch): (i32, i32, i32, i32)|
     -> Result<HWND, Win32Error> {
        // SAFETY: `hwnd` is the live parent created above; `id` is passed
        // as the child identifier through the HMENU parameter, as Win32
        // specifies for child windows.
        let c = unsafe {
            CreateWindowExW(
                ex,
                class,
                text,
                WS_CHILD | WS_VISIBLE | style,
                px(cx),
                px(cy),
                px(cw),
                px(ch),
                Some(hwnd),
                Some(HMENU(id as isize as *mut core::ffi::c_void)),
                Some(hinst),
                None,
            )
        }
        .map_err(|_| Win32Error::Failed)?;
        // SAFETY: `c` is a live child window; WM_SETFONT takes the font
        // handle in wParam and a redraw flag in lParam.
        unsafe {
            SendMessageW(
                c,
                WM_SETFONT,
                Some(WPARAM(font.0.0 as usize)),
                Some(LPARAM(1)),
            );
        }
        Ok(c)
    };

    let inner_w = CLIENT_W - 2 * MARGIN;
    let built = (|| {
        child(
            w!("STATIC"),
            w!("請貼上你的 Claude API 金鑰："),
            WINDOW_STYLE(0),
            WINDOW_EX_STYLE(0),
            -1,
            (MARGIN, MARGIN, inner_w, 20),
        )?;
        let edit = child(
            w!("EDIT"),
            PCWSTR::null(),
            WS_TABSTOP | WINDOW_STYLE((ES_PASSWORD | ES_AUTOHSCROLL) as u32),
            WS_EX_CLIENTEDGE,
            ID_EDIT,
            (MARGIN, MARGIN + 24, inner_w, 26),
        )?;
        child(
            w!("STATIC"),
            w!("金鑰會存入 Windows 認證管理員，不會進入設定頁。\n儲存後只顯示末四碼，無法再查看。"),
            WINDOW_STYLE(0),
            WINDOW_EX_STYLE(0),
            -1,
            (MARGIN, MARGIN + 60, inner_w, 40),
        )?;
        let ok = child(
            w!("BUTTON"),
            w!("確定"),
            WS_TABSTOP | WS_DISABLED | WINDOW_STYLE(BS_DEFPUSHBUTTON as u32),
            WINDOW_EX_STYLE(0),
            ID_OK,
            (
                CLIENT_W - MARGIN - 2 * BTN_W - 8,
                CLIENT_H - MARGIN - BTN_H,
                BTN_W,
                BTN_H,
            ),
        )?;
        child(
            w!("BUTTON"),
            w!("取消"),
            WS_TABSTOP | WINDOW_STYLE(BS_PUSHBUTTON as u32),
            WINDOW_EX_STYLE(0),
            ID_CANCEL,
            (
                CLIENT_W - MARGIN - BTN_W,
                CLIENT_H - MARGIN - BTN_H,
                BTN_W,
                BTN_H,
            ),
        )?;
        Ok::<_, Win32Error>((edit, ok))
    })();
    let (edit, ok) = match built {
        Ok(v) => v,
        Err(e) => {
            // SAFETY: `hwnd` is live and owned by this thread.
            let _ = unsafe { DestroyWindow(hwnd) };
            return Err(e);
        }
    };

    STATE.with(|s| {
        *s.borrow_mut() = Some(State {
            edit,
            ok,
            result: None,
            done: false,
        });
    });

    // SAFETY: all handles are live windows created by this thread above.
    unsafe {
        SendMessageW(
            edit,
            EM_SETLIMITTEXT,
            Some(WPARAM(MAX_CHARS)),
            Some(LPARAM(0)),
        );
        // Keep the key out of screenshots and screen sharing.
        if SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE).is_err() {
            let _ = SetWindowDisplayAffinity(hwnd, WDA_MONITOR);
        }
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = SetForegroundWindow(hwnd);
        let _ = SetFocus(Some(edit));
    }

    run_loop(hwnd);

    let state = STATE.with(|s| s.borrow_mut().take());
    drop(font);
    Ok(state.and_then(|s| s.result))
}

fn run_loop(hwnd: HWND) {
    let mut msg = MSG::default();
    loop {
        if STATE.with(|s| s.borrow().as_ref().is_none_or(|s| s.done)) {
            break;
        }
        // SAFETY: `msg` is a valid MSG out-parameter.
        let r = unsafe { GetMessageW(&mut msg, None, 0, 0) };
        if r.0 <= 0 {
            // WM_QUIT or an error: close the dialog as cancelled.
            finish(hwnd, None);
            break;
        }
        // SAFETY: `msg` was filled by GetMessageW; `hwnd` is our dialog.
        // IsDialogMessageW gives Tab / Enter / Esc dialog behaviour.
        unsafe {
            if !IsDialogMessageW(hwnd, &msg).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    }
}

fn instance() -> Result<HINSTANCE, Win32Error> {
    // SAFETY: None asks for the executable's own module handle.
    let m = unsafe { GetModuleHandleW(PCWSTR::null()) }.map_err(|_| Win32Error::Failed)?;
    Ok(HINSTANCE(m.0))
}

fn register_class(hinst: HINSTANCE) -> Result<(), Win32Error> {
    static REGISTERED: OnceLock<bool> = OnceLock::new();
    let ok = *REGISTERED.get_or_init(|| {
        // SAFETY: IDC_ARROW is a system cursor resource id.
        let cursor = unsafe { LoadCursorW(None, IDC_ARROW) }.unwrap_or_default();
        let wc = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(wndproc),
            hInstance: hinst,
            hCursor: cursor,
            // SAFETY: system colour brushes need no cleanup.
            hbrBackground: unsafe { GetSysColorBrush(COLOR_WINDOW) },
            lpszClassName: CLASS,
            ..Default::default()
        };
        // SAFETY: `wc` is fully initialized; the class name is static.
        unsafe { RegisterClassExW(&wc) != 0 }
    });
    if ok { Ok(()) } else { Err(Win32Error::Failed) }
}

fn system_dpi() -> i32 {
    // SAFETY: screen DC obtained and released in this block.
    unsafe {
        let hdc: HDC = GetDC(None);
        if hdc.is_invalid() {
            return 96;
        }
        let dpi = GetDeviceCaps(Some(hdc), LOGPIXELSY);
        ReleaseDC(None, hdc);
        if dpi > 0 { dpi } else { 96 }
    }
}

/// The system message font (Segoe UI / Microsoft JhengHei UI on zh-TW).
fn message_font() -> HFONT {
    let mut ncm = NONCLIENTMETRICSW {
        cbSize: size_of::<NONCLIENTMETRICSW>() as u32,
        ..Default::default()
    };
    // SAFETY: `ncm` is a correctly sized NONCLIENTMETRICSW out-parameter.
    let got = unsafe {
        SystemParametersInfoW(
            SPI_GETNONCLIENTMETRICS,
            ncm.cbSize,
            Some((&raw mut ncm).cast()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    };
    if got.is_err() {
        return HFONT::default();
    }
    // SAFETY: `lfMessageFont` is a valid LOGFONTW filled in above.
    unsafe { CreateFontIndirectW(&ncm.lfMessageFont) }
}

/// Top-left corner that centres a `w` × `h` window in the work area.
fn centered(w: i32, h: i32) -> (i32, i32) {
    let mut area = RECT::default();
    // SAFETY: `area` is a RECT out-parameter, as SPI_GETWORKAREA expects.
    let got = unsafe {
        SystemParametersInfoW(
            SPI_GETWORKAREA,
            0,
            Some((&raw mut area).cast()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    };
    if got.is_err() {
        return (100, 100);
    }
    (
        area.left + (area.right - area.left - w) / 2,
        area.top + (area.bottom - area.top - h) / 3,
    )
}

fn handles() -> Option<(HWND, HWND)> {
    STATE.with(|s| s.try_borrow().ok()?.as_ref().map(|s| (s.edit, s.ok)))
}

extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_COMMAND => {
            let id = (wparam.0 & 0xFFFF) as i32;
            let code = ((wparam.0 >> 16) & 0xFFFF) as u32;
            match id {
                ID_EDIT if code == EN_CHANGE => update_ok(),
                ID_OK => accept(hwnd),
                ID_CANCEL => {
                    scrub_edit();
                    finish(hwnd, None);
                }
                _ => {}
            }
            LRESULT(0)
        }
        WM_CLOSE => {
            scrub_edit();
            finish(hwnd, None);
            LRESULT(0)
        }
        // Enter = OK (IsDialogMessageW asks for the default button).
        DM_GETDEFID => LRESULT(((DC_HASDEFID as isize) << 16) | ID_OK as isize),
        WM_CTLCOLORSTATIC => {
            // Labels on the window background colour.
            let hdc = HDC(wparam.0 as *mut core::ffi::c_void);
            // SAFETY: wParam of WM_CTLCOLORSTATIC is the label's DC, valid
            // for this message; system colour brushes need no cleanup.
            unsafe {
                SetBkColor(hdc, COLORREF(GetSysColor(COLOR_WINDOW)));
                SetTextColor(hdc, COLORREF(GetSysColor(COLOR_WINDOWTEXT)));
                LRESULT(GetSysColorBrush(COLOR_WINDOW).0 as isize)
            }
        }
        // SAFETY: default handling for every other message, with the
        // arguments Windows passed in.
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

/// Enable OK once there are enough characters. Reads only the length.
fn update_ok() {
    let Some((edit, ok)) = handles() else {
        return;
    };
    // SAFETY: both are live child windows of the dialog on this thread.
    unsafe {
        let n = GetWindowTextLengthW(edit);
        let _ = EnableWindow(ok, n >= MIN_CHARS);
    }
}

fn accept(hwnd: HWND) {
    let Some((edit, _)) = handles() else {
        return;
    };
    // SAFETY: `edit` is a live child window on this thread.
    let len = unsafe { GetWindowTextLengthW(edit) };
    if len < MIN_CHARS {
        return;
    }
    let mut buf: Zeroizing<Vec<u16>> = Zeroizing::new(vec![0u16; len as usize + 1]);
    // SAFETY: `buf` has room for `len` units plus the terminator.
    let n = unsafe { GetWindowTextW(edit, &mut buf) }.max(0) as usize;
    let units = trim_ascii_ws(&buf[..n.min(buf.len())]);
    // Exact UTF-8 length first, so the String never reallocates (a
    // reallocation would leave an unzeroized copy in freed memory).
    let exact: usize = char::decode_utf16(units.iter().copied())
        .map(|c| c.unwrap_or(char::REPLACEMENT_CHARACTER).len_utf8())
        .sum();
    let mut s = String::with_capacity(exact);
    for c in char::decode_utf16(units.iter().copied()) {
        s.push(c.unwrap_or(char::REPLACEMENT_CHARACTER));
    }
    let key = SecretString::from(s);
    drop(buf);
    scrub_edit();
    finish(hwnd, Some(key));
}

/// Overwrite the edit control's text before the window goes away.
fn scrub_edit() {
    let Some((edit, _)) = handles() else {
        return;
    };
    // SAFETY: `edit` is a live child window on this thread.
    let len = unsafe { GetWindowTextLengthW(edit) }.max(0) as usize;
    let mut filler = vec![u16::from(b'x'); len];
    filler.push(0);
    // SAFETY: `filler` is NUL-terminated and outlives the call.
    unsafe {
        let _ = SetWindowTextW(edit, PCWSTR(filler.as_ptr()));
        let _ = SetWindowTextW(edit, w!(""));
    }
}

fn finish(hwnd: HWND, result: Option<SecretString>) {
    let first = STATE.with(|s| match s.try_borrow_mut() {
        Ok(mut g) => match g.as_mut() {
            Some(st) if !st.done => {
                st.done = true;
                st.result = result;
                true
            }
            _ => false,
        },
        Err(_) => false,
    });
    if first {
        // SAFETY: `hwnd` is our dialog, destroyed once (guarded by `done`).
        let _ = unsafe { DestroyWindow(hwnd) };
    }
}
