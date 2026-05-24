//! VQF oracle replay test.
//!
//! Asserts max abs error per quaternion component < 1e-9 vs reference output.

use imu_fusion::Vqf;
use serde_json::Value;

#[test]
fn vqf_oracle_replay_matches_reference() {
    let json = std::fs::read_to_string("fixtures/vqf_oracle.json").expect("fixture missing");
    let v: Value = serde_json::from_str(&json).unwrap();

    let fs = v["fs"].as_f64().unwrap();
    let n = v["n"].as_u64().unwrap() as usize;
    let ts = 1.0 / fs;

    let read_vec3 = |key: &str| -> Vec<[f64; 3]> {
        v[key]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| {
                let r = row.as_array().unwrap();
                [
                    r[0].as_f64().unwrap(),
                    r[1].as_f64().unwrap(),
                    r[2].as_f64().unwrap(),
                ]
            })
            .collect()
    };

    let gyr = read_vec3("gyr");
    let acc = read_vec3("acc");
    let mag = read_vec3("mag");

    let expected_q6: Vec<[f64; 4]> = v["expected"]["quat6d"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| {
            let r = row.as_array().unwrap();
            [
                r[0].as_f64().unwrap(),
                r[1].as_f64().unwrap(),
                r[2].as_f64().unwrap(),
                r[3].as_f64().unwrap(),
            ]
        })
        .collect();

    let mut vqf = Vqf::new(ts);
    let mut max_err = 0.0_f64;
    for i in 0..n {
        vqf.update(gyr[i], acc[i], Some(mag[i]));
        let got = vqf.quat_6d();
        let sign = (got[0] * expected_q6[i][0]
            + got[1] * expected_q6[i][1]
            + got[2] * expected_q6[i][2]
            + got[3] * expected_q6[i][3])
            .signum();
        for (c, g) in got.iter().enumerate() {
            let err = (sign * g - expected_q6[i][c]).abs();
            if err > max_err {
                max_err = err;
            }
        }
    }
    assert!(
        max_err < 1e-9,
        "max VQF quat6d error {max_err} exceeds 1e-9 oracle tolerance"
    );
}
