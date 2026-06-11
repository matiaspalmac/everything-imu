//! eimu remote protocol v1 — wire parse/encode.
//!
//! Envelope: `45 49 4D 55` ("EIMU") + version u8 + msg u8, then a
//! little-endian payload. One message per UDP datagram.

use device_traits::{DeviceKind, ImuSample, ResetKind};

pub const MAGIC: [u8; 4] = *b"EIMU";
pub const VERSION: u8 = 1;
pub const DEFAULT_PORT: u16 = 9320;

pub const MSG_HELLO: u8 = 0x01;
pub const MSG_HELLO_ACK: u8 = 0x02;
pub const MSG_ANNOUNCE: u8 = 0x03;
pub const MSG_REMOVE: u8 = 0x04;
pub const MSG_IMU: u8 = 0x05;
pub const MSG_BATTERY: u8 = 0x06;
pub const MSG_BUTTON: u8 = 0x07;
pub const MSG_RUMBLE: u8 = 0x08;

pub const KIND_PHONE: u8 = 1;
pub const KIND_WATCH: u8 = 2;
pub const KIND_JOYCON2_L: u8 = 3;
pub const KIND_JOYCON2_R: u8 = 4;
pub const KIND_PRO_CONTROLLER_2: u8 = 5;
pub const KIND_HOPX: u8 = 6;
pub const KIND_DUALSENSE: u8 = 7;
pub const KIND_DUALSHOCK4: u8 = 8;
pub const KIND_JOYCON_L: u8 = 9;
pub const KIND_JOYCON_R: u8 = 10;
pub const KIND_PRO_CONTROLLER: u8 = 11;
pub const KIND_GAMEPAD: u8 = 12;

const HEADER_LEN: usize = 6;
const SAMPLE_LEN: usize = 8 + 12 + 12 + 1 + 12; // 45

#[derive(Debug, Clone, PartialEq)]
pub struct Announce {
    pub handle: u16,
    pub kind: DeviceKind,
    pub mac: [u8; 6],
    pub has_mag: bool,
    pub has_battery: bool,
    pub has_rumble: bool,
    pub rate_hz: u16,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RemoteMsg {
    Hello {
        uuid: [u8; 16],
        name: String,
    },
    Announce(Announce),
    Remove {
        handle: u16,
    },
    Imu {
        handle: u16,
        samples: Vec<ImuSample>,
    },
    Battery {
        handle: u16,
        fraction: f32,
        charging: bool,
    },
    Button {
        handle: u16,
        reset: ResetKind,
    },
}

fn kind_from_wire(b: u8) -> Option<DeviceKind> {
    match b {
        KIND_PHONE => Some(DeviceKind::Phone),
        KIND_WATCH => Some(DeviceKind::Watch),
        KIND_JOYCON2_L => Some(DeviceKind::JoyCon2L),
        KIND_JOYCON2_R => Some(DeviceKind::JoyCon2R),
        KIND_PRO_CONTROLLER_2 => Some(DeviceKind::ProController2),
        KIND_HOPX => Some(DeviceKind::Hopx),
        KIND_DUALSENSE => Some(DeviceKind::DualSense),
        KIND_DUALSHOCK4 => Some(DeviceKind::DualShock4),
        KIND_JOYCON_L => Some(DeviceKind::JoyConL),
        KIND_JOYCON_R => Some(DeviceKind::JoyConR),
        KIND_PRO_CONTROLLER => Some(DeviceKind::ProController),
        KIND_GAMEPAD => Some(DeviceKind::Gamepad),
        _ => None,
    }
}

pub fn parse(buf: &[u8]) -> Option<RemoteMsg> {
    if buf.len() < HEADER_LEN || buf[..4] != MAGIC || buf[4] != VERSION {
        return None;
    }
    let msg = buf[5];
    let p = &buf[HEADER_LEN..];
    let u16le = |b: &[u8], o: usize| u16::from_le_bytes([b[o], b[o + 1]]);
    let f32le = |b: &[u8], o: usize| f32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]);
    match msg {
        MSG_HELLO => {
            if p.len() < 17 {
                return None;
            }
            let mut uuid = [0u8; 16];
            uuid.copy_from_slice(&p[..16]);
            let n = p[16] as usize;
            if p.len() < 17 + n {
                return None;
            }
            let name = std::str::from_utf8(&p[17..17 + n]).ok()?.to_string();
            Some(RemoteMsg::Hello { uuid, name })
        }
        MSG_ANNOUNCE => {
            if p.len() < 15 {
                return None;
            }
            let handle = u16le(p, 0);
            let kind = kind_from_wire(p[2])?;
            let mut mac = [0u8; 6];
            mac.copy_from_slice(&p[3..9]);
            let n = p[14] as usize;
            if p.len() < 15 + n {
                return None;
            }
            let name = std::str::from_utf8(&p[15..15 + n]).ok()?.to_string();
            Some(RemoteMsg::Announce(Announce {
                handle,
                kind,
                mac,
                has_mag: p[9] != 0,
                has_battery: p[10] != 0,
                has_rumble: p[11] != 0,
                rate_hz: u16le(p, 12),
                name,
            }))
        }
        MSG_REMOVE => {
            if p.len() < 2 {
                return None;
            }
            Some(RemoteMsg::Remove {
                handle: u16le(p, 0),
            })
        }
        MSG_IMU => {
            if p.len() < 3 {
                return None;
            }
            let handle = u16le(p, 0);
            let count = p[2] as usize;
            if p.len() < 3 + count * SAMPLE_LEN {
                return None;
            }
            let mut samples = Vec::with_capacity(count);
            for i in 0..count {
                let off = 3 + i * SAMPLE_LEN;
                let ts = u64::from_le_bytes([
                    p[off],
                    p[off + 1],
                    p[off + 2],
                    p[off + 3],
                    p[off + 4],
                    p[off + 5],
                    p[off + 6],
                    p[off + 7],
                ]);
                let gx = f32le(p, off + 8);
                let gy = f32le(p, off + 12);
                let gz = f32le(p, off + 16);
                let ax = f32le(p, off + 20);
                let ay = f32le(p, off + 24);
                let az = f32le(p, off + 28);
                let has_mag = p[off + 32] != 0;
                let mx = f32le(p, off + 33);
                let my = f32le(p, off + 37);
                let mz = f32le(p, off + 41);

                samples.push(ImuSample {
                    timestamp_us: ts,
                    gyro: [gx, gy, gz],
                    accel: [ax, ay, az],
                    mag: if has_mag { Some([mx, my, mz]) } else { None },
                });
            }
            Some(RemoteMsg::Imu { handle, samples })
        }
        MSG_BATTERY => {
            if p.len() < 7 {
                return None;
            }
            Some(RemoteMsg::Battery {
                handle: u16le(p, 0),
                fraction: f32le(p, 2),
                charging: p[6] != 0,
            })
        }
        MSG_BUTTON => {
            if p.len() < 3 {
                return None;
            }
            let reset = if p[2] == 0 {
                ResetKind::Yaw
            } else {
                ResetKind::Full
            };
            Some(RemoteMsg::Button {
                handle: u16le(p, 0),
                reset,
            })
        }
        _ => None,
    }
}

pub fn encode_hello_ack() -> Vec<u8> {
    let mut buf = Vec::with_capacity(7);
    buf.extend_from_slice(&MAGIC);
    buf.push(VERSION);
    buf.push(MSG_HELLO_ACK);
    buf.push(VERSION); // server version
    buf
}

pub fn encode_rumble(handle: u16, intensity: f32) -> Vec<u8> {
    let mut buf = Vec::with_capacity(12);
    buf.extend_from_slice(&MAGIC);
    buf.push(VERSION);
    buf.push(MSG_RUMBLE);
    buf.extend_from_slice(&handle.to_le_bytes());
    buf.extend_from_slice(&intensity.to_le_bytes());
    buf
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hdr(msg: u8) -> Vec<u8> {
        vec![0x45, 0x49, 0x4D, 0x55, 0x01, msg]
    }

    #[test]
    fn rejects_bad_magic_and_version() {
        assert!(parse(&[0u8; 6]).is_none());
        let mut b = hdr(MSG_HELLO);
        b[4] = 2; // wrong version
        b.extend_from_slice(&[0u8; 17]);
        assert!(parse(&b).is_none());
    }

    #[test]
    fn parses_hello() {
        let mut b = hdr(MSG_HELLO);
        b.extend_from_slice(&[0xAA; 16]);
        b.push(4);
        b.extend_from_slice(b"Pixe");
        let Some(RemoteMsg::Hello { uuid, name }) = parse(&b) else {
            panic!("expected hello");
        };
        assert_eq!(uuid, [0xAA; 16]);
        assert_eq!(name, "Pixe");
    }

    #[test]
    fn parses_announce() {
        let mut b = hdr(MSG_ANNOUNCE);
        b.extend_from_slice(&7u16.to_le_bytes());
        b.push(KIND_JOYCON2_R);
        b.extend_from_slice(&[1, 2, 3, 4, 5, 6]);
        b.push(0); // has_mag
        b.push(1); // has_battery
        b.push(1); // has_rumble
        b.extend_from_slice(&120u16.to_le_bytes());
        b.push(3);
        b.extend_from_slice(b"JC2");
        let Some(RemoteMsg::Announce(a)) = parse(&b) else {
            panic!("expected announce");
        };
        assert_eq!(a.handle, 7);
        assert_eq!(a.kind, device_traits::DeviceKind::JoyCon2R);
        assert_eq!(a.mac, [1, 2, 3, 4, 5, 6]);
        assert!(!a.has_mag && a.has_battery && a.has_rumble);
        assert_eq!(a.rate_hz, 120);
        assert_eq!(a.name, "JC2");
    }

    #[test]
    fn gamepad_kinds_map_to_device_kinds() {
        use device_traits::DeviceKind;
        let cases = [
            (KIND_DUALSENSE, DeviceKind::DualSense),
            (KIND_DUALSHOCK4, DeviceKind::DualShock4),
            (KIND_JOYCON_L, DeviceKind::JoyConL),
            (KIND_JOYCON_R, DeviceKind::JoyConR),
            (KIND_PRO_CONTROLLER, DeviceKind::ProController),
            (KIND_GAMEPAD, DeviceKind::Gamepad),
        ];
        for (wire, expected) in cases {
            let mut b = hdr(MSG_ANNOUNCE);
            b.extend_from_slice(&1u16.to_le_bytes());
            b.push(wire);
            b.extend_from_slice(&[0u8; 6]);
            b.extend_from_slice(&[0, 1, 1]);
            b.extend_from_slice(&200u16.to_le_bytes());
            b.push(0);
            let Some(RemoteMsg::Announce(a)) = parse(&b) else {
                panic!("expected announce for wire kind {wire}");
            };
            assert_eq!(a.kind, expected);
        }
    }

    #[test]
    fn announce_with_unknown_kind_is_dropped() {
        let mut b = hdr(MSG_ANNOUNCE);
        b.extend_from_slice(&7u16.to_le_bytes());
        b.push(0xEE);
        b.extend_from_slice(&[0u8; 6 + 3 + 2 + 1]);
        assert!(parse(&b).is_none());
    }

    #[test]
    fn parses_imu_burst_with_and_without_mag() {
        let mut b = hdr(MSG_IMU);
        b.extend_from_slice(&0u16.to_le_bytes());
        b.push(2);
        // sample 1: mag present
        b.extend_from_slice(&1_000u64.to_le_bytes());
        for v in [0.1f32, 0.2, 0.3, 9.8, 0.0, 0.1] {
            b.extend_from_slice(&v.to_le_bytes());
        }
        b.push(1);
        for v in [20.0f32, -30.0, 40.0] {
            b.extend_from_slice(&v.to_le_bytes());
        }
        // sample 2: no mag (mag bytes still on wire, ignored)
        b.extend_from_slice(&2_000u64.to_le_bytes());
        for v in [0.4f32, 0.5, 0.6, 0.0, 9.8, 0.0] {
            b.extend_from_slice(&v.to_le_bytes());
        }
        b.push(0);
        b.extend_from_slice(&[0u8; 12]);
        let Some(RemoteMsg::Imu { handle, samples }) = parse(&b) else {
            panic!("expected imu");
        };
        assert_eq!(handle, 0);
        assert_eq!(samples.len(), 2);
        assert_eq!(samples[0].timestamp_us, 1_000);
        assert_eq!(samples[0].gyro, [0.1, 0.2, 0.3]);
        assert_eq!(samples[0].accel, [9.8, 0.0, 0.1]);
        assert_eq!(samples[0].mag, Some([20.0, -30.0, 40.0]));
        assert_eq!(samples[1].mag, None);
        assert_eq!(samples[1].timestamp_us, 2_000);
    }

    #[test]
    fn imu_with_truncated_sample_is_dropped() {
        let mut b = hdr(MSG_IMU);
        b.extend_from_slice(&0u16.to_le_bytes());
        b.push(1);
        b.extend_from_slice(&[0u8; 44]); // one byte short
        assert!(parse(&b).is_none());
    }

    #[test]
    fn parses_battery_button_remove() {
        let mut b = hdr(MSG_BATTERY);
        b.extend_from_slice(&3u16.to_le_bytes());
        b.extend_from_slice(&0.75f32.to_le_bytes());
        b.push(1);
        let Some(RemoteMsg::Battery {
            handle,
            fraction,
            charging,
        }) = parse(&b)
        else {
            panic!("battery");
        };
        assert_eq!((handle, fraction, charging), (3, 0.75, true));

        let mut b = hdr(MSG_BUTTON);
        b.extend_from_slice(&3u16.to_le_bytes());
        b.push(1);
        let Some(RemoteMsg::Button { handle: 3, reset }) = parse(&b) else {
            panic!("button");
        };
        assert_eq!(reset, device_traits::ResetKind::Full);

        let mut b = hdr(MSG_REMOVE);
        b.extend_from_slice(&9u16.to_le_bytes());
        assert!(matches!(parse(&b), Some(RemoteMsg::Remove { handle: 9 })));
    }

    #[test]
    fn encodes_hello_ack_and_rumble() {
        assert_eq!(
            encode_hello_ack(),
            vec![0x45, 0x49, 0x4D, 0x55, 0x01, MSG_HELLO_ACK, 0x01]
        );
        let r = encode_rumble(2, 0.5);
        assert_eq!(&r[..6], &[0x45, 0x49, 0x4D, 0x55, 0x01, MSG_RUMBLE]);
        assert_eq!(&r[6..8], &2u16.to_le_bytes());
        assert_eq!(&r[8..12], &0.5f32.to_le_bytes());
    }
}
