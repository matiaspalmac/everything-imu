//! Detect Pro Controller clones (8BitDo Pro 2 / Ultimate 2 in Switch mode).
//!
//! These controllers spoof Nintendo's USB VID:PID (`057E:2009`) and speak the
//! Pro Controller HID protocol but lack a populated factory SPI calibration
//! block at offsets `0x6020` / `0x8026`. The existing SPI parser already
//! detects the zero/0xFF flash case and falls back to nominal coefficients —
//! this module only labels the device for UI surfacing and tracing.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProControllerVariant {
    Nintendo,
    BitDo8Pro2,
    BitDo8Ultimate2,
    UnknownClone,
}

impl ProControllerVariant {
    pub fn is_clone(self) -> bool {
        !matches!(self, Self::Nintendo)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Nintendo => "Pro Controller",
            Self::BitDo8Pro2 => "8BitDo Pro 2 (Switch mode)",
            Self::BitDo8Ultimate2 => "8BitDo Ultimate 2 (Switch mode)",
            Self::UnknownClone => "Pro Controller clone",
        }
    }
}

/// Heuristic classification from the USB serial string.
///
/// Nintendo's first-party Pro Controllers report a 12-hex-character serial that
/// is the wired MAC of the unit, e.g. `98B6E9F5C311`. 8BitDo firmware reports
/// vendor-specific strings (`E4:17:D8:...`, `8BitDo-Pro2-xxxx`, etc.) so any
/// non-12-hex serial is treated as a clone. Empty serials (some BT stacks
/// withhold them) are conservatively treated as Nintendo.
pub fn classify_serial(serial: &str) -> ProControllerVariant {
    if serial.is_empty() {
        return ProControllerVariant::Nintendo;
    }
    let s_lower = serial.to_ascii_lowercase();
    if s_lower.contains("pro2") || s_lower.contains("pro 2") {
        return ProControllerVariant::BitDo8Pro2;
    }
    if s_lower.contains("ultimate2") || s_lower.contains("ultimate 2") {
        return ProControllerVariant::BitDo8Ultimate2;
    }
    if s_lower.contains("8bitdo") {
        return ProControllerVariant::UnknownClone;
    }
    if is_nintendo_style_mac(serial) {
        ProControllerVariant::Nintendo
    } else {
        ProControllerVariant::UnknownClone
    }
}

fn is_nintendo_style_mac(serial: &str) -> bool {
    serial.len() == 12 && serial.chars().all(|c| c.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nintendo_pro_controller_serial() {
        assert_eq!(
            classify_serial("98B6E9F5C311"),
            ProControllerVariant::Nintendo
        );
    }

    #[test]
    fn empty_serial_assumed_nintendo() {
        assert_eq!(classify_serial(""), ProControllerVariant::Nintendo);
    }

    #[test]
    fn pro2_name_detected() {
        assert_eq!(
            classify_serial("8BitDo-Pro2-1234"),
            ProControllerVariant::BitDo8Pro2
        );
    }

    #[test]
    fn ultimate2_name_detected() {
        assert_eq!(
            classify_serial("8BitDo-Ultimate2-abcd"),
            ProControllerVariant::BitDo8Ultimate2
        );
    }

    #[test]
    fn non_hex_serial_flagged_unknown_clone() {
        assert_eq!(
            classify_serial("E4:17:D8:01:23:45"),
            ProControllerVariant::UnknownClone
        );
    }

    #[test]
    fn clone_flag_helper() {
        assert!(!ProControllerVariant::Nintendo.is_clone());
        assert!(ProControllerVariant::BitDo8Pro2.is_clone());
        assert!(ProControllerVariant::UnknownClone.is_clone());
    }

    #[test]
    fn labels_are_distinct() {
        assert_ne!(
            ProControllerVariant::Nintendo.label(),
            ProControllerVariant::BitDo8Pro2.label(),
        );
    }
}
