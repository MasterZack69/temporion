//! temporiond — the Temporion daemon.
//!
//! Reads CPU (Tctl), NVIDIA GPU and NVMe temperatures and streams them to
//! stdout, one line per sample, as space-separated whole degrees Celsius:
//!
//!     <cpu> <gpu> <disk0> <disk1> ...
//!
//! A field is `-` when that sensor is currently unavailable (e.g. the dGPU is
//! runtime-suspended). The GNOME extension reads these lines asynchronously,
//! so gnome-shell never does blocking sensor I/O on its own main loop.
//!
//! Environment:
//!   TEMPORION_INTERVAL_MS  sample interval in ms (default 2000)
//!   TEMPORION_GPU_ALWAYS   set to "1" to query the dGPU even while suspended
//!                          (default: skip when suspended, to save battery)

mod sensors;

use std::io::Write;
use std::time::Duration;

use sensors::Sensors;

fn env_u64(key: &str, default: u64) -> u64 {
    match std::env::var(key) {
        Ok(v) => v.trim().parse().unwrap_or(default),
        Err(_) => default,
    }
}

fn main() {
    let interval = Duration::from_millis(env_u64("TEMPORION_INTERVAL_MS", 2000).max(100));
    let gpu_always = matches!(std::env::var("TEMPORION_GPU_ALWAYS").as_deref(), Ok("1"));

    let mut sensors = Sensors::discover(gpu_always);
    sensors.log_summary();

    // Reused output buffer: no per-sample heap allocation on the hot path.
    let mut buf = String::with_capacity(48);
    let stdout = std::io::stdout();
    let mut out = stdout.lock();

    loop {
        buf.clear();
        sensors.sample(&mut buf);
        buf.push('\n');
        // If the reader (the extension) is gone, write/flush fails; exit quietly.
        if out.write_all(buf.as_bytes()).is_err() || out.flush().is_err() {
            break;
        }
        std::thread::sleep(interval);
    }
}
