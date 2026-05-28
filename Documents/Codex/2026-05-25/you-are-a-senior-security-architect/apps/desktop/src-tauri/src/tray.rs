//! System tray, autostart registration, and auto-lock timer.

use crate::state::AppState;
use tauri::{
    menu::{MenuBuilder, MenuItemBuilder, PredefinedMenuItem, SubmenuBuilder},
    tray::TrayIconBuilder,
    Manager,
};

/// Returns true when enough time has passed since the last vault access.
///
/// `last_access` and `now` are Unix timestamps (seconds).
/// `threshold_secs` is the configured lock delay.
pub(crate) fn should_lock(last_access: i64, now: i64, threshold_secs: i64) -> bool {
    now - last_access >= threshold_secs
}

/// Build and register the system tray icon and menu.
pub fn setup_tray(app: &tauri::App) -> Result<(), tauri::Error> {
    let open_item  = MenuItemBuilder::new("Åbn").id("open").build(app)?;
    let lock_item  = MenuItemBuilder::new("Lås vault").id("lock").build(app)?;
    let sep1       = PredefinedMenuItem::separator(app)?;

    let al1   = MenuItemBuilder::new("1 min").id("al_1").build(app)?;
    let al5   = MenuItemBuilder::new("5 min").id("al_5").build(app)?;
    let al15  = MenuItemBuilder::new("15 min (standard)").id("al_15").build(app)?;
    let al60  = MenuItemBuilder::new("60 min").id("al_60").build(app)?;
    let al240 = MenuItemBuilder::new("240 min").id("al_240").build(app)?;
    let al_never = MenuItemBuilder::new("Aldrig").id("al_never").build(app)?;

    let al_sub = SubmenuBuilder::new(app, "Auto-lock")
        .item(&al1).item(&al5).item(&al15)
        .item(&al60).item(&al240).item(&al_never)
        .build()?;

    let sep2      = PredefinedMenuItem::separator(app)?;
    let quit_item = MenuItemBuilder::new("Afslut").id("quit").build(app)?;

    let menu = MenuBuilder::new(app)
        .item(&open_item)
        .item(&lock_item)
        .item(&sep1)
        .item(&al_sub)
        .item(&sep2)
        .item(&quit_item)
        .build()?;

    TrayIconBuilder::new()
        .menu(&menu)
        .tooltip("ESPASS")
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| handle_menu_event(app, event.id().as_ref()))
        .build(app)?;

    Ok(())
}

fn handle_menu_event(app: &tauri::AppHandle, id: &str) {
    match id {
        "open" => {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.set_focus();
            }
        }
        "lock" => {
            let state = app.state::<AppState>();
            if let Ok(mut s) = state.secrets.lock() { s.lock(); }
            if let Ok(mut s) = state.session.lock() { *s = None; }
            let _ = state.lock_notify_tx.send(());
        }
        "quit" => app.exit(0),
        al if al.starts_with("al_") => {
            let minutes: Option<u32> = match al {
                "al_1"    => Some(1),
                "al_5"    => Some(5),
                "al_15"   => Some(15),
                "al_60"   => Some(60),
                "al_240"  => Some(240),
                "al_never" => None,
                _ => return,
            };
            let state = app.state::<AppState>();
            state.set_autolock_minutes(minutes);
        }
        _ => {}
    }
}

/// Spawn the background auto-lock timer task.
/// Wakes every 30 seconds and locks the vault if the configured timeout has elapsed.
pub fn start_autolock_task(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
        interval.tick().await; // skip immediate first tick
        loop {
            interval.tick().await;
            let state = app.state::<AppState>();

            let is_unlocked = state.secrets.lock().map_or(false, |s| s.is_unlocked());
            if !is_unlocked {
                continue;
            }

            let threshold_secs: i64 = {
                let minutes = state.autolock_minutes.lock().ok().as_deref().copied().flatten();
                match minutes {
                    None => continue, // never auto-lock
                    Some(m) => m as i64 * 60,
                }
            };

            let last_access = state
                .last_vault_access
                .load(std::sync::atomic::Ordering::Acquire);
            let now = time::OffsetDateTime::now_utc().unix_timestamp();

            if should_lock(last_access, now, threshold_secs) {
                if let Ok(mut s) = state.secrets.lock() { s.lock(); }
                if let Ok(mut s) = state.session.lock() { *s = None; }
                let _ = state.lock_notify_tx.send(());
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_autolock_when_threshold_exceeded() {
        assert!(should_lock(0, 1000, 900));  // 1000 - 0 = 1000 > 900 — clearly exceeded
    }

    #[test]
    fn should_not_autolock_when_recently_accessed() {
        assert!(!should_lock(950, 1000, 900));
    }

    #[test]
    fn exact_boundary_triggers_lock() {
        assert!(should_lock(100, 1000, 900));  // 1000 - 100 = 900 == 900 — exact boundary, should lock (>=)
    }
}
