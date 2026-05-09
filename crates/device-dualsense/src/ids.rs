//! Sony VID + DualSense / DualShock 4 PID constants.

use device_traits::DeviceKind;

pub const SONY_VID: u16 = 0x054C;

pub mod pid {
    pub const DUALSENSE: u16 = 0x0CE6;
    pub const DUALSENSE_EDGE: u16 = 0x0DF2;
    pub const DUALSHOCK_4_V1: u16 = 0x05C4;
    pub const DUALSHOCK_4_V2: u16 = 0x09CC;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControllerKind {
    DualSense,
    DualSenseEdge,
    DualShock4,
}

impl ControllerKind {
    pub fn from_pid(pid: u16) -> Option<Self> {
        match pid {
            pid::DUALSENSE => Some(Self::DualSense),
            pid::DUALSENSE_EDGE => Some(Self::DualSenseEdge),
            pid::DUALSHOCK_4_V1 | pid::DUALSHOCK_4_V2 => Some(Self::DualShock4),
            _ => None,
        }
    }

    pub fn into_device_kind(self) -> DeviceKind {
        match self {
            Self::DualSense => DeviceKind::DualSense,
            Self::DualSenseEdge => DeviceKind::DualSenseEdge,
            Self::DualShock4 => DeviceKind::DualShock4,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pid_to_kind() {
        assert_eq!(
            ControllerKind::from_pid(0x0CE6),
            Some(ControllerKind::DualSense),
        );
        assert_eq!(
            ControllerKind::from_pid(0x0DF2),
            Some(ControllerKind::DualSenseEdge),
        );
        assert_eq!(
            ControllerKind::from_pid(0x05C4),
            Some(ControllerKind::DualShock4),
        );
        assert_eq!(
            ControllerKind::from_pid(0x09CC),
            Some(ControllerKind::DualShock4),
        );
        assert_eq!(ControllerKind::from_pid(0x1234), None);
    }
}
