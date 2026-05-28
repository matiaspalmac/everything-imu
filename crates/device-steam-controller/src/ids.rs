//! Steam Controller USB identity constants.

pub const VALVE_VID: u16 = 0x28DE;
pub const PID_WIRED: u16 = 0x1102;
pub const PID_DONGLE: u16 = 0x1142;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SteamControllerTransport {
    UsbWired,
    UsbDongle,
}

impl SteamControllerTransport {
    pub fn from_pid(pid: u16) -> Option<Self> {
        match pid {
            PID_WIRED => Some(Self::UsbWired),
            PID_DONGLE => Some(Self::UsbDongle),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::UsbWired => "Steam Controller (wired)",
            Self::UsbDongle => "Steam Controller (wireless dongle)",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pid_decoding() {
        assert_eq!(
            SteamControllerTransport::from_pid(PID_WIRED),
            Some(SteamControllerTransport::UsbWired)
        );
        assert_eq!(
            SteamControllerTransport::from_pid(PID_DONGLE),
            Some(SteamControllerTransport::UsbDongle)
        );
        assert_eq!(SteamControllerTransport::from_pid(0x1205), None);
    }

    #[test]
    fn distinct_labels() {
        assert_ne!(
            SteamControllerTransport::UsbWired.label(),
            SteamControllerTransport::UsbDongle.label()
        );
    }
}
