//! VID/PID constants + ControllerKind detection.

use device_traits::DeviceKind;

pub const JOYCON_VID: u16 = 0x057E;

pub mod pid {
    pub const JOYCON_L: u16 = 0x2006;
    pub const JOYCON_R: u16 = 0x2007;
    pub const PRO_CONTROLLER: u16 = 0x2009;
    pub const CHARGING_GRIP: u16 = 0x200E;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControllerKind {
    JoyConL,
    JoyConR,
    ProController,
    ChargingGripL,
    ChargingGripR,
}

impl ControllerKind {
    pub fn from_pid(pid: u16) -> Option<Self> {
        match pid {
            pid::JOYCON_L => Some(Self::JoyConL),
            pid::JOYCON_R => Some(Self::JoyConR),
            pid::PRO_CONTROLLER => Some(Self::ProController),
            pid::CHARGING_GRIP => None,
            _ => None,
        }
    }

    pub fn from_device_info_byte(b: u8) -> Option<Self> {
        match b {
            0x01 => Some(Self::JoyConL),
            0x02 => Some(Self::JoyConR),
            0x03 => Some(Self::ProController),
            _ => None,
        }
    }

    pub fn into_device_kind(self) -> DeviceKind {
        match self {
            Self::JoyConL => DeviceKind::JoyConL,
            Self::JoyConR => DeviceKind::JoyConR,
            Self::ProController => DeviceKind::ProController,
            Self::ChargingGripL => DeviceKind::ChargingGripL,
            Self::ChargingGripR => DeviceKind::ChargingGripR,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pid_to_kind() {
        assert_eq!(
            ControllerKind::from_pid(0x2006),
            Some(ControllerKind::JoyConL)
        );
        assert_eq!(
            ControllerKind::from_pid(0x2007),
            Some(ControllerKind::JoyConR)
        );
        assert_eq!(
            ControllerKind::from_pid(0x2009),
            Some(ControllerKind::ProController)
        );
        assert_eq!(ControllerKind::from_pid(0x200E), None);
    }

    #[test]
    fn device_info_byte_to_kind() {
        assert_eq!(
            ControllerKind::from_device_info_byte(0x01),
            Some(ControllerKind::JoyConL)
        );
        assert_eq!(
            ControllerKind::from_device_info_byte(0x02),
            Some(ControllerKind::JoyConR)
        );
        assert_eq!(
            ControllerKind::from_device_info_byte(0x03),
            Some(ControllerKind::ProController)
        );
        assert_eq!(ControllerKind::from_device_info_byte(0xFF), None);
    }
}
