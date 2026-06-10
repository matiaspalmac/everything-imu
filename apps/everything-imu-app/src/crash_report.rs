//! Opt-in crash reporting via Sentry.
//!
//! Initialization is gated on the `crash_report_enabled` setting AND the
//! `EVERYTHING_IMU_SENTRY_DSN` environment variable being set at build /
//! launch time. No telemetry leaves the machine until both are true,
//! which matches the privacy copy shown to the user in Settings.
//!
//! The transport runs in a guard that must be kept alive for the
//! lifetime of the process; `init` leaks the guard intentionally so the
//! caller can simply call this once at boot and forget about it.

#[cfg(feature = "crash-reporting")]
pub fn init_if_opted_in(enabled: bool) {
    if !enabled {
        return;
    }
    let dsn = match std::env::var("EVERYTHING_IMU_SENTRY_DSN") {
        Ok(v) if !v.is_empty() => v,
        _ => return,
    };
    let guard = sentry::init((
        dsn,
        sentry::ClientOptions {
            release: sentry::release_name!(),
            send_default_pii: false,
            attach_stacktrace: true,
            ..Default::default()
        },
    ));
    // Sentry's guard must outlive the process; leaking it is the
    // idiomatic "init once at boot" pattern from the SDK README.
    Box::leak(Box::new(guard));
    tracing::info!("crash reporting enabled");
}

#[cfg(not(feature = "crash-reporting"))]
pub fn init_if_opted_in(_enabled: bool) {
    // Build configured without the `crash-reporting` feature — there is
    // nothing to do. The Settings toggle still persists; flipping it on
    // simply has no effect until a build that bundles Sentry ships.
}
