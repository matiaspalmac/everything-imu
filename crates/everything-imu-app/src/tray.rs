//! Tray icon with dynamic tooltip + nav / broadcast actions.

use tauri::menu::{MenuBuilder, MenuItemBuilder, PredefinedMenuItem};
use tauri::tray::{TrayIcon, TrayIconBuilder};
use tauri::{AppHandle as TauriAppHandle, Manager};

pub struct TrayHandle(pub TrayIcon);

pub fn init_tray(app: &TauriAppHandle) -> tauri::Result<()> {
    let show = MenuItemBuilder::with_id("show", "Show window").build(app)?;
    let dashboard = MenuItemBuilder::with_id("nav-dashboard", "Open Dashboard").build(app)?;
    let connection = MenuItemBuilder::with_id("nav-connection", "Open Connection").build(app)?;
    let yaw = MenuItemBuilder::with_id("reset-yaw", "Broadcast Yaw Reset").build(app)?;
    let full = MenuItemBuilder::with_id("reset-full", "Broadcast Full Reset").build(app)?;
    let quit = MenuItemBuilder::with_id("quit", "Quit").build(app)?;
    let menu = MenuBuilder::new(app)
        .items(&[
            &show,
            &PredefinedMenuItem::separator(app)?,
            &dashboard,
            &connection,
            &PredefinedMenuItem::separator(app)?,
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
        .on_menu_event(|app, ev| match ev.id.as_ref() {
            "show" => focus_main(app),
            "nav-dashboard" => navigate(app, "/"),
            "nav-connection" => navigate(app, "/connection"),
            "reset-yaw" => broadcast_reset(app, crate::dto::ResetKindDto::Yaw),
            "reset-full" => broadcast_reset(app, crate::dto::ResetKindDto::Full),
            "quit" => app.exit(0),
            _ => {}
        })
        .build(app)?;
    app.manage(TrayHandle(tray));
    Ok(())
}

fn focus_main(app: &TauriAppHandle) {
    if let Some(w) = app.get_webview_window("main") {
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

#[allow(dead_code)]
pub fn update_tray_tooltip(app: &TauriAppHandle, count: usize) {
    if let Some(handle) = app.try_state::<TrayHandle>() {
        let _ = handle
            .0
            .set_tooltip(Some(format!("everything-imu — {count} devices connected")));
    }
}
