//! CPU temperature: AMD k10temp `Tctl` sensor (Ryzen 7 7435HS).

use std::fs;
use std::path::{Path, PathBuf};

use super::hwmon;
use super::milli_to_deg;

pub struct Cpu {
    input: PathBuf,
}

impl Cpu {
    /// Find the k10temp hwmon device and its `Tctl` input node.
    pub fn discover() -> Option<Cpu> {
        let base = Path::new("/sys/class/hwmon");
        for entry in fs::read_dir(base).ok()?.flatten() {
            let dir = entry.path();
            if hwmon::device_name(&dir).as_deref() != Some("k10temp") {
                continue;
            }
            if let Some(input) = hwmon::labeled_input(&dir, "Tctl") {
                return Some(Cpu { input });
            }
        }
        None
    }

    pub fn read(&self) -> Option<i32> {
        hwmon::read_millideg(&self.input).map(milli_to_deg)
    }
}
