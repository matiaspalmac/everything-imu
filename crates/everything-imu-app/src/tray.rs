//! Tray icon: tooltip + nav / broadcast actions + window restore.
//!
//! Click semantics
//! ---------------
//! * **Left click** on the icon → restore + focus the main window (works
//!   when it's hidden, minimized, or buried behind other windows).
//! * **Menu → Quit** → hard `app.exit(0)` after explicitly closing the
//!   main window, so the tray icon vanishes immediately on Windows.
//!
//! Window-close behavior is owned by `register_window_handlers` in
//! `lib.rs`, not the tray itself — that's where the `close_to_tray`
//! setting is honored.

use tauri::menu::{MenuBuilder, MenuItemBuilder, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle as TauriAppHandle, Manager};

pub struct TrayHandle(pub TrayIcon);

pub fn init_tray(app: &TauriAppHandle) -> tauri::Result<()> {
    let show = MenuItemBuilder::with_id("show", "Show window").build(app)?;
    let dashboard = MenuItemBuilder::with_id("nav-dashboard", "Open Dashboard").build(app)?;
    let connection = MenuItemBuilder::with_id("nav-connection", "Open Connection").build(app)?;
    let toggle = MenuItemBuilder::with_id("toggle-bridge", "Pause / resume bridge").build(app)?;
    let yaw = MenuItemBuilder::with_id("reset-yaw", "Broadcast Yaw Reset").build(app)?;
    let full = MenuItemBuilder::with_id("reset-full", "Broadcast Full Reset").build(app)?;
    let quit = MenuItemBuilder::with_id("quit", "Quit everything-imu").build(app)?;
    let menu = MenuBuilder::new(app)
        .items(&[
            &show,
            &PredefinedMenuItem::separator(app)?,
            &dashboard,
            &connection,
            &PredefinedMenuItem::separator(app)?,
            &toggle,
            &yaw,
            &full,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ])
        .build()?;
    let icon = app
        .default_window_icon()
        .cloned()
        .expect("default window icon");
    let tray = TrayIconBuilder::with_id("main")
        .icon(icon)
        .tooltip("everything-imu — 0 devices")
        .menu(&menu)
        // Some platforms swallow the left-click as a menu-open gesture
        // when this is true; we want left-click to mean "show window."
        .show_menu_on_left_click(false)
        .on_menu_event(|app, ev| match ev.id.as_ref() {
            "show" => focus_main(app),
            "nav-dashboard" => navigate(app, "/"),
            "nav-connection" => navigate(app, "/connection"),
            "toggle-bridge" => toggle_bridge(app),
            "reset-yaw" => broadcast_reset(app, crate::dto::ResetKindDto::Yaw),
            "reset-full" => broadcast_reset(app, crate::dto::ResetKindDto::Full),
            "quit" => quit_app(app),
            _ => {}
        })
        .on_tray_icon_event(|tray, ev| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = ev
            {
                focus_main(tray.app_handle());
            }
        })
        .build(app)?;
    app.manage(TrayHandle(tray));
    Ok(())
}

/// Show + unminimize + focus the main window. Used by tray clicks AND by
/// the single-instance plugin when a second launch is intercepted — same
/// behavior either way: bring the existing window forward.
pub fn focus_main(app: &TauriAppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

/// Push a route into the React Router history without a full reload.
/// Tray menu items use this so users can deep-link from the system tray.
fn navigate(app: &TauriAppHandle, path: &str) {
    focus_main(app);
    if let Some(w) = app.get_webview_window("main") {
        let js = format!(
            "window.history.pushState(null, '', '{path}'); \
             window.dispatchEvent(new PopStateEvent('popstate'));"
        );
        let _ = w.eval(&js);
    }
}

/// Flip the global emission pause flag. Mirrors what the UI kill-switch
/// and `Ctrl+Shift+B` do, but available even when the window is hidden.
fn toggle_bridge(app: &TauriAppHandle) {
    if let Some(handle) = app.try_state::<crate::state::AppHandle>() {
        let now_paused = handle.state.is_paused();
        handle.state.set_paused(!now_paused);
    }
}

/// Fire one user-action packet per known device. Useful when the app is
/// minimized and the user wants to recenter without bringing the window
/// up first.
fn broadcast_reset(app: &TauriAppHandle, kind: crate::dto::ResetKindDto) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let handle = match app.try_state::<crate::state::AppHandle>() {
            Some(h) => h,
            None => return,
        };
        let metas = handle.state.device_metadata_snapshot().await;
        for m in metas {
            let id = device_traits::DeviceId {
                mac: m.id.mac,
                serial: m.id.serial.clone(),
            };
            if let Err(e) = handle.state.request_reset(&id, kind.into()).await {
                tracing::warn!(error = %e, mac = ?id.mac, "tray broadcast reset failed");
            }
        }
    });
}

/// Clean shutdown: explicitly destroy the main window first so the
/// close-to-tray handler doesn't intercept and re-hide it, then exit.
/// Without the explicit destroy the tray icon can linger on Windows
/// for a few seconds after exit; with it, it disappears instantly.
fn quit_app(app: &TauriAppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.destroy();
    }
    app.exit(0);
}

pub fn update_tray_tooltip(app: &TauriAppHandle, count: usize, low_battery: usize) {
    if let Some(handle) = app.try_state::<TrayHandle>() {
        let base = if count == 1 {
            "everything-imu — 1 device connected".to_string()
        } else {
            format!("everything-imu — {count} devices connected")
        };
        // The OS tray tooltip can't render a real badge cross-platform,
        // so we append a single warning line when at least one device is
        // below the low-battery threshold. Hidden when nothing is low so
        // healthy operation stays quiet.
        let label = if low_battery > 0 {
            format!("{base}\n⚠ {low_battery} low battery")
        } else {
            base
        };
        let _ = handle.0.set_tooltip(Some(label));
    }
}
