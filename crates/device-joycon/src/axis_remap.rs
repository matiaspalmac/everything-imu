//! Per-ControllerKind axis sign flips (chip mounted mirrored on JoyConR).

use crate::ids::ControllerKind;

#[inline]
pub fn apply(kind: ControllerKind, v: [f32; 3]) -> [f32; 3] {
    match kind {
        ControllerKind::JoyConR | ControllerKind::ChargingGripR => [v[0], -v[1], -v[2]],
        ControllerKind::JoyConL | ControllerKind::ProController | ControllerKind::ChargingGripL => {
            v
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jc_l_passthrough() {
        assert_eq!(
            apply(ControllerKind::JoyConL, [1.0, 2.0, 3.0]),
            [1.0, 2.0, 3.0]
        );
    }

    #[test]
    fn jc_r_negates_y_z() {
        assert_eq!(
            apply(ControllerKind::JoyConR, [1.0, 2.0, 3.0]),
            [1.0, -2.0, -3.0]
        );
    }

    #[test]
    fn pro_passthrough() {
        assert_eq!(
            apply(ControllerKind::ProController, [1.0, 2.0, 3.0]),
            [1.0, 2.0, 3.0]
        );
    }

    #[test]
    fn grip_l_passthrough_grip_r_negates() {
        assert_eq!(
            apply(ControllerKind::ChargingGripL, [1.0, 2.0, 3.0]),
            [1.0, 2.0, 3.0]
        );
        assert_eq!(
            apply(ControllerKind::ChargingGripR, [1.0, 2.0, 3.0]),
            [1.0, -2.0, -3.0]
        );
    }
}
