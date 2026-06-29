//! Linux udev rules installer.
//!
//! Joy-Con, DualSense and PSMove enumerate as raw HID devices on Linux,
//! and the kernel default-denies non-root user access to `/dev/hidraw*`.
//! Without the rules below, `hidapi` only sees nothing and the bridge
//! sits empty wondering why no controller ever shows up. The user can
//! install them manually but an "Install rules" button is the friendlier UX.
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
    use std::io::Write;
    use std::process::Stdio;

    if Command::new("which")
        .arg("pkexec")
        .output()
        .map(|o| !o.status.success())
        .unwrap_or(true)
    {
        return Err(InstallError::NoPkexec);
    }

    // Pipe the ruleset straight into the privileged shell over stdin and let
    // the root shell write the destination itself. There is deliberately no
    // intermediate staging file: a predictable path in a world-writable dir
    // (e.g. /tmp) is a symlink/TOCTOU vector — a local attacker could swap the
    // staged file for a symlink and have the root copy clobber an arbitrary
    // path. The destination is a fixed root-owned constant, so nothing here is
    // attacker-influenceable.
    let shell_cmd = format!(
        "cat > {dst} && chmod 0644 {dst} && udevadm control --reload-rules && udevadm trigger",
        dst = TARGET_PATH,
    );
    let mut child = Command::new("pkexec")
        .args(["sh", "-c", &shell_cmd])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    {
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| std::io::Error::other("pkexec stdin unavailable"))?;
        stdin.write_all(RULES.as_bytes())?;
        // Dropping `stdin` here closes the pipe so `cat` sees EOF.
    }
    let out = child.wait_with_output()?;
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
