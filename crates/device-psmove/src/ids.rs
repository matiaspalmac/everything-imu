//! Sony VID + PS Move PID constants.

use device_traits::DeviceKind;

pub const SONY_VID: u16 = 0x054C;

pub mod pid {
    /// PlayStation Move ZCM1 (PS3 era).
    pub const ZCM1: u16 = 0x03D5;
    /// PlayStation Move ZCM2 (PS4 refresh).
    pub const ZCM2: u16 = 0x0C5E;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControllerKind {
    Zcm1,
    Zcm2,
}

impl ControllerKind {
    pub fn from_pid(pid: u16) -> Option<Self> {
        match pid {
            pid::ZCM1 => Some(Self::Zcm1),
            pid::ZCM2 => Some(Self::Zcm2),
            _ => None,
        }
    }

    pub fn into_device_kind(self) -> DeviceKind {
        match self {
            Self::Zcm1 => DeviceKind::PsMoveZcm1,
            Self::Zcm2 => DeviceKind::PsMoveZcm2,
        }
    }

    pub fn has_magnetometer(self) -> bool {
        matches!(self, Self::Zcm1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pid_to_kind() {
        assert_eq!(ControllerKind::from_pid(0x03D5), Some(ControllerKind::Zcm1));
        assert_eq!(ControllerKind::from_pid(0x0C5E), Some(ControllerKind::Zcm2));
        assert_eq!(ControllerKind::from_pid(0x1234), None);
        assert!(ControllerKind::Zcm1.has_magnetometer());
        assert!(!ControllerKind::Zcm2.has_magnetometer());
    }
}
