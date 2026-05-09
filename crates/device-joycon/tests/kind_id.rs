use device_joycon::ids::ControllerKind;

#[test]
fn subcmd_0x02_overrides_pid_disagreement() {
    let pid_kind = ControllerKind::from_pid(0x2009).unwrap();
    let info_kind = ControllerKind::from_device_info_byte(0x02).unwrap();
    assert_eq!(pid_kind, ControllerKind::ProController);
    assert_eq!(info_kind, ControllerKind::JoyConR);

    let final_kind = if pid_kind != info_kind {
        info_kind
    } else {
        pid_kind
    };
    assert_eq!(final_kind, ControllerKind::JoyConR);
}

#[test]
fn subcmd_0x02_agrees_with_pid_no_change() {
    let pid_kind = ControllerKind::from_pid(0x2007).unwrap();
    let info_kind = ControllerKind::from_device_info_byte(0x02).unwrap();
    assert_eq!(pid_kind, info_kind);
}
