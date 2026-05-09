//! Decode raw button bytes from report 0x30 into [`device_traits::ButtonState`].

use crate::ids::ControllerKind;
use crate::report::ButtonsRaw;
use device_traits::ButtonState;

const BIT_HOME: u8 = 0x10;
const BIT_CAPTURE: u8 = 0x20;

pub fn decode(kind: ControllerKind, b: ButtonsRaw) -> ButtonState {
    let home = (b.shared & BIT_HOME) != 0;
    let capture = (b.shared & BIT_CAPTURE) != 0;
    match kind {
        ControllerKind::JoyConL | ControllerKind::ChargingGripL => {
            ButtonState::CaptureOnly { pressed: capture }
        }
        ControllerKind::JoyConR | ControllerKind::ChargingGripR | ControllerKind::ProController => {
            ButtonState::HomeOrCapture {
                home_pressed: home,
                capture_pressed: capture,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn b(shared: u8) -> ButtonsRaw {
        ButtonsRaw {
            low: 0,
            high: 0,
            shared,
        }
    }

    #[test]
    fn jc_l_capture_only() {
        let s = decode(ControllerKind::JoyConL, b(BIT_CAPTURE));
        match s {
            ButtonState::CaptureOnly { pressed } => assert!(pressed),
            other => panic!("expected CaptureOnly, got {other:?}"),
        }
    }

    #[test]
    fn jc_r_home_only() {
        let s = decode(ControllerKind::JoyConR, b(BIT_HOME));
        match s {
            ButtonState::HomeOrCapture {
                home_pressed,
                capture_pressed,
            } => {
                assert!(home_pressed);
                assert!(!capture_pressed);
            }
            other => panic!("expected HomeOrCapture, got {other:?}"),
        }
    }

    #[test]
    fn pro_home_and_capture() {
        let s = decode(ControllerKind::ProController, b(BIT_HOME | BIT_CAPTURE));
        match s {
            ButtonState::HomeOrCapture {
                home_pressed,
                capture_pressed,
            } => {
                assert!(home_pressed);
                assert!(capture_pressed);
            }
            other => panic!("expected HomeOrCapture, got {other:?}"),
        }
    }
}
