//! Pre-flight probe for the platform webview runtime.
//!
//! The window is drawn by the system webview: WebView2 on Windows, WebKitGTK
//! on Linux. A Windows machine can be missing WebView2 entirely — the NSIS
//! installer downloads it during setup, so an offline install or a build that
//! was simply copied over ends up without it.
//!
//! When it is missing, the failure only surfaces after `setup` has already
//! opened the database and started the device pipeline, the tray and the
//! haptic bridge. The user sees an error, the window never appears, and the
//! process is left running in the background with no UI to close it from.
//!
//! Probing before any of that starts keeps that state unreachable: the user
//! gets one dialog naming what is missing, and the process exits with nothing
//! left behind.

#[cfg(windows)]
use windows::core::{HSTRING, PCWSTR};
#[cfg(windows)]
use windows::Win32::UI::Shell::ShellExecuteW;
#[cfg(windows)]
use windows::Win32::UI::WindowsAndMessaging::{
    MessageBoxW, IDYES, MB_ICONERROR, MB_OK, MB_SETFOREGROUND, MB_TOPMOST, MB_YESNO, SW_SHOWNORMAL,
};

/// Download page for the Evergreen WebView2 runtime.
pub const WEBVIEW2_DOWNLOAD_URL: &str = "https://developer.microsoft.com/microsoft-edge/webview2/";

/// Title used for every pre-window dialog. Tauri is not up yet at this point,
/// so these are plain native message boxes rather than app windows.
#[cfg(windows)]
const DIALOG_TITLE: &str = "everything-imu";

/// Body of the dialog shown when the webview runtime is missing.
pub fn missing_runtime_message(detail: &str) -> String {
    format!(
        "everything-imu needs the Microsoft Edge WebView2 runtime to draw its window, \
         and this computer does not have it.\n\n\
         Install the WebView2 runtime, then start everything-imu again.\n\n\
         Open the download page now?\n\n\
         ({detail})"
    )
}

/// Body of the dialog shown when the app fails to start for any other reason.
pub fn startup_failure_message(detail: &str) -> String {
    format!(
        "everything-imu could not open its window and is shutting down.\n\n\
         ({detail})"
    )
}

/// Probe the webview runtime and call off the launch when it is unusable.
///
/// Runs as the very first thing in `run()`, before any state is built, so
/// exiting here leaves no window, thread, socket or tray icon behind.
pub fn ensure_available() {
    #[cfg(windows)]
    if let Err(e) = tauri::webview_version() {
        let detail = e.to_string();
        // The logging subscriber is installed in `setup`, which never runs on
        // this path, so write the reason where a console build can see it.
        eprintln!("webview runtime unavailable: {detail}");
        if prompt_yes_no(&missing_runtime_message(&detail)) {
            open_url(WEBVIEW2_DOWNLOAD_URL);
        }
        std::process::exit(1);
    }
}

/// Report a fatal startup failure and take the whole process down with it.
///
/// Reached when the event loop itself returns an error, which happens after
/// `setup` has started the background pipeline. Panicking out of `run` would
/// leave those threads deciding when (or whether) the process dies, so exit
/// explicitly instead.
pub fn fail_startup(detail: &str) -> ! {
    tracing::error!(error = %detail, "app runtime failed to start");
    eprintln!("everything-imu failed to start: {detail}");
    alert(&startup_failure_message(detail));
    std::process::exit(1);
}

#[cfg(windows)]
fn alert(body: &str) {
    // SAFETY: both strings outlive the call, and a null owner window is the
    // documented way to show an ownerless modal box.
    unsafe {
        MessageBoxW(
            None,
            &HSTRING::from(body),
            &HSTRING::from(DIALOG_TITLE),
            MB_OK | MB_ICONERROR | MB_SETFOREGROUND | MB_TOPMOST,
        );
    }
}

#[cfg(not(windows))]
fn alert(_body: &str) {
    // Linux ships the webview as a shared library the loader resolves at
    // startup, so a missing runtime never reaches this code path; the
    // terminal output from the caller is all there is to say.
}

#[cfg(windows)]
fn prompt_yes_no(body: &str) -> bool {
    // SAFETY: see `alert`.
    let answer = unsafe {
        MessageBoxW(
            None,
            &HSTRING::from(body),
            &HSTRING::from(DIALOG_TITLE),
            MB_YESNO | MB_ICONERROR | MB_SETFOREGROUND | MB_TOPMOST,
        )
    };
    answer == IDYES
}

#[cfg(windows)]
fn open_url(url: &str) {
    // SAFETY: the strings outlive the call and the shell copies what it needs.
    unsafe {
        ShellExecuteW(
            None,
            &HSTRING::from("open"),
            &HSTRING::from(url),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_runtime_message_names_the_runtime_and_the_cause() {
        let msg = missing_runtime_message("os error 2");
        assert!(msg.contains("WebView2"), "{msg}");
        assert!(msg.contains("os error 2"), "{msg}");
    }

    #[test]
    fn startup_failure_message_carries_the_cause() {
        let msg = startup_failure_message("runtime creation failed");
        assert!(msg.contains("runtime creation failed"), "{msg}");
    }

    #[test]
    fn download_url_points_at_the_evergreen_runtime() {
        assert!(WEBVIEW2_DOWNLOAD_URL.starts_with("https://"));
        assert!(WEBVIEW2_DOWNLOAD_URL.contains("webview2"));
    }
}
