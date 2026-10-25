//! System tray icon and menu (docs/mockups/desktop.html, Windows).

use std::sync::Arc;

use keepsay_core::Mode;
use keepsay_core::config::WINDOWS_HOTKEYS;
use tauri::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager, Wry};

use crate::app::{self, Shared};
use crate::lock;
use crate::session::Phase;

const MODES: [(Mode, &str, &str); 4] = [
    (Mode::Raw, "mode-raw", "原文"),
    (Mode::Light, "mode-light", "標點"),
    (Mode::Polish, "mode-polish", "潤稿"),
    (Mode::Formal, "mode-formal", "正式"),
];

#[derive(Clone)]
pub struct TrayHandles {
    header: MenuItem<Wry>,
    hint: MenuItem<Wry>,
    modes: Vec<(Mode, CheckMenuItem<Wry>)>,
    budget: MenuItem<Wry>,
}

pub fn build(app: &AppHandle, shared: &Arc<Shared>) -> tauri::Result<()> {
    let header = MenuItem::with_id(app, "header", "keepsay・就緒", false, None::<&str>)?;
    let hint = MenuItem::with_id(app, "hint", "", false, None::<&str>)?;
    let label = MenuItem::with_id(app, "mode-label", "模式", false, None::<&str>)?;
    let mut modes = Vec::new();
    for (mode, id, text) in MODES {
        modes.push((
            mode,
            CheckMenuItem::with_id(app, id, text, true, false, None::<&str>)?,
        ));
    }
    let budget = MenuItem::with_id(app, "budget", "", false, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "設定…", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "結束 keepsay", true, None::<&str>)?;
    let sep = || PredefinedMenuItem::separator(app);
    let (s1, s2, s3) = (sep()?, sep()?, sep()?);

    let menu = Menu::new(app)?;
    menu.append(&header)?;
    menu.append(&hint)?;
    menu.append(&s1)?;
    menu.append(&label)?;
    for (_, item) in &modes {
        menu.append(item)?;
    }
    menu.append(&s2)?;
    menu.append(&budget)?;
    menu.append(&s3)?;
    menu.append(&settings)?;
    menu.append(&quit)?;

    let mut tray = TrayIconBuilder::with_id("keepsay")
        .tooltip("keepsay")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(on_menu);
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;

    *lock(&shared.tray) = Some(TrayHandles {
        header,
        hint,
        modes,
        budget,
    });
    refresh(shared);
    Ok(())
}

fn on_menu(app: &AppHandle, event: MenuEvent) {
    let shared = app.state::<Arc<Shared>>().inner().clone();
    let id = event.id().as_ref();
    if let Some((mode, _, _)) = MODES.iter().find(|(_, mid, _)| *mid == id) {
        set_mode(&shared, *mode);
        return;
    }
    match id {
        "settings" => app::show_settings(app),
        "quit" => app.exit(0),
        _ => {}
    }
}

fn set_mode(shared: &Shared, mode: Mode) {
    {
        let mut cfg = lock(&shared.config);
        cfg.mode = mode;
        if cfg.save(&shared.paths.config).is_err() {
            tracing::warn!("config save failed");
        }
    }
    refresh(shared);
}

/// Bring the menu in line with the current state.
pub fn refresh(shared: &Shared) {
    let mode = lock(&shared.config).mode;
    let phase = lock(&shared.session).phase();
    let hotkey = lock(&shared.hotkeys)
        .view
        .as_ref()
        .map(|v| v.polish.clone())
        .unwrap_or_else(|| WINDOWS_HOTKEYS.polish.to_owned());
    let has_key = shared.key_view().has_key;
    let budget = shared.budget_view();

    // Clone the handles out: the setters below hop to the main thread, which
    // must never wait on a lock held here.
    let Some(t) = lock(&shared.tray).clone() else {
        return;
    };
    let status = match phase {
        Phase::Idle => "就緒",
        Phase::Recording(_) => "錄音中",
        Phase::Processing(_) => "處理中",
    };
    let _ = t.header.set_text(format!("keepsay・{status}"));
    let _ = t.hint.set_text(format!("按 {hotkey} 開始／停止說話"));
    for (m, item) in &t.modes {
        let _ = item.set_checked(*m == mode);
        let _ = item.set_enabled(has_key || *m == Mode::Raw);
    }
    let line = if !has_key {
        "未設定金鑰・只用原文".to_owned()
    } else if budget.ledger_unreadable {
        "預算紀錄無法讀取・只用原文".to_owned()
    } else {
        match budget.hard_limit_usd {
            Some(h) => format!("本月 US${:.2} ／ {:.2}", budget.estimated_usd, h),
            None => format!("本月 US${:.2}", budget.estimated_usd),
        }
    };
    let _ = t.budget.set_text(line);
}
