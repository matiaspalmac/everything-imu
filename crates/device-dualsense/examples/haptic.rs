//! Live output-report check for a connected DualSense.
//!
//! Drives the first paired DualSense through a short RGB-lightbar + rumble
//! sequence so the output path can be confirmed on real hardware.
//!
//! Run with: `cargo run -p device-dualsense --example haptic`

fn main() {
    match device_dualsense::diagnostics::haptic_test() {
        Ok(()) => println!("haptic test completed"),
        Err(e) => {
            eprintln!("haptic test failed: {e}");
            std::process::exit(1);
        }
    }
}
