//! Linux udev rules installer.
//!
//! Joy-Con, DualSense and PSMove enumerate as raw HID devices on Linux,
//! and the kernel default-denies non-root user access to `/dev/hidraw*`.
//! Without the rules below, `hidapi` only sees nothing and the bridge
//! sits empty wondering why no controller ever shows up. The user can
//! install them manually but a "Install rules" button is the same UX
//! Steam Big Picture, slimevr-wrangler and SDL2 all expose.
//!
//! The Tauri command is a no-op on Windows / macOS so the UI can call
//! it unconditionally and surface the platform check to the user.

#[cfg(target_os = "linux")]
use std::process::Command;

/// Embedded udev ruleset granting user-mode access to the HID devices we
/// support. Matches the VID/PIDs used by the device crates; keep these
/// two in sync. Permissions: 0660 + uaccess so the local seat owner
/// gets RW without root.
pub const RULES: &str = r#"# everything-imu — let the local user open Joy-Con / DualSense / PSMove HID nodes.
# Nintendo (Joy-Con L/R, Pro, NSO N64, Joy-Con 2, Pro 2)
KERNEL=="hidraw*", ATTRS{idVendor}=="057e", ATTRS{idProduct}=="2006", MODE="0660", TAG+="uaccess"
KERNEL=="hidraw*", ATTRS{idVendor}=="057e", ATTRS{idProduct}=="2007", MODE="0660", TAG+="uaccess"
KERNEL=="hidraw*", ATTRS{idVendor}=="057e", ATTRS{idProduct}=="2009", MODE="0660", TAG+="uaccess"
KERNEL=="hidraw*", ATTRS{idVendor}=="057e", ATTRS{idProduct}=="2017", MODE="0660", TAG+="uaccess"
KERNEL=="hidraw*", ATTRS{idVendor}=="057e", ATTRS{idProduct}=="2066", MODE="0660", TAG+="uaccess"
KERNEL=="hidraw*", ATTRS{idVendor}=="057e", ATTRS{idProduct}=="2067", MODE="0660", TAG+="uaccess"
# Sony (DualShock 4, DualSense, DualSense Edge, PSMove)
KERNEL=="hidraw*", ATTRS{idVendor}=="054c", ATTRS{idProduct}=="05c4", MODE="0660", TAG+="uaccess"
KERNEL=="hidraw*", ATTRS{idVendor}=="054c", ATTRS{idProduct}=="09cc", MODE="0660", TAG+="uaccess"
KERNEL=="hidraw*", ATTRS{idVendor}=="054c", ATTRS{idProduct}=="0ce6", MODE="0660", TAG+="uaccess"
KERNEL=="hidraw*", ATTRS{idVendor}=="054c", ATTRS{idProduct}=="0df2", MODE="0660", TAG+="uaccess"
KERNEL=="hidraw*", ATTRS{idVendor}=="054c", ATTRS{idProduct}=="03d5", MODE="0660", TAG+="uaccess"
"#;

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
const TARGET_PATH: &str = "/etc/udev/rules.d/99-everything-imu.rules";

#[derive(Debug, thiserror::Error)]
pub enum InstallError {
    #[error("udev rule installation is Linux-only")]
    UnsupportedPlatform,
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("pkexec failed (exit {code}): {stderr}")]
    Pkexec { code: i32, stderr: String },
    #[error("pkexec not found in PATH — install policykit and retry")]
    NoPkexec,
}

#[cfg(target_os = "linux")]
pub fn install() -> Result<String, InstallError> {
    // Stage the rules in /tmp so pkexec only needs to copy + reload —
    // keeps the privileged shell invocation minimal and auditable.
    let staged = std::path::PathBuf::from("/tmp/99-everything-imu.rules");
    std::fs::write(&staged, RULES)?;

    if Command::new("which")
        .arg("pkexec")
        .output()
        .map(|o| !o.status.success())
        .unwrap_or(true)
    {
        return Err(InstallError::NoPkexec);
    }

    let shell_cmd = format!(
        "install -m 0644 {src} {dst} && udevadm control --reload-rules && udevadm trigger",
        src = staged.display(),
        dst = TARGET_PATH,
    );
    let out = Command::new("pkexec")
        .args(["sh", "-c", &shell_cmd])
        .output()?;
    if !out.status.success() {
        return Err(InstallError::Pkexec {
            code: out.status.code().unwrap_or(-1),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        });
    }
    Ok(format!(
        "Installed udev rules to {TARGET_PATH}. Reconnect controllers."
    ))
}

#[cfg(not(target_os = "linux"))]
pub fn install() -> Result<String, InstallError> {
    Err(InstallError::UnsupportedPlatform)
}
