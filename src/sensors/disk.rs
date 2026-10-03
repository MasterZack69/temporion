//! NVMe SSD temperatures via the `nvme` hwmon `Composite` sensor.
//!
//! No smartctl, no root, no subprocess: the kernel nvme driver exposes the
//! composite temperature directly in sysfs.

use std::fs;
use std::path::{Path, PathBuf};

use super::hwmon;
use super::milli_to_deg;

pub struct Disk {
    input: PathBuf,
    node: String,  // e.g. "nvme0"
    model: String, // e.g. "Micron ..." / "WD ..."
}

impl Disk {
    /// Discover every NVMe composite-temperature sensor, ordered by controller
    /// index (nvme0 -> disk0, nvme1 -> disk1, ...).
    pub fn discover_all() -> Vec<Disk> {
        let mut disks: Vec<(u32, Disk)> = Vec::new();
        let base = Path::new("/sys/class/hwmon");
        let read_dir = match fs::read_dir(base) {
            Ok(rd) => rd,
            Err(_) => return Vec::new(),
        };
        for entry in read_dir.flatten() {
            let dir = entry.path();
            if hwmon::device_name(&dir).as_deref() != Some("nvme") {
                continue;
            }
            let input = match hwmon::labeled_input(&dir, "Composite") {
                Some(p) => p,
                None => continue,
            };
            let index = nvme_index(&dir).unwrap_or(u32::MAX);
            let node = match index {
                u32::MAX => "nvme?".to_owned(),
                n => format!("nvme{n}"),
            };
            let model = read_model(index).unwrap_or_else(|| "unknown".to_owned());
            disks.push((index, Disk { input, node, model }));
        }
        disks.sort_by_key(|(index, _)| *index);
        disks.into_iter().map(|(_, disk)| disk).collect()
    }

    pub fn read(&self) -> Option<i32> {
        hwmon::read_millideg(&self.input).map(milli_to_deg)
    }

    pub fn node(&self) -> &str {
        &self.node
    }

    pub fn model(&self) -> &str {
        &self.model
    }
}

/// Resolve the nvme controller index from a hwmon dir's `device` symlink.
fn nvme_index(dir: &Path) -> Option<u32> {
    let target = fs::canonicalize(dir.join("device")).ok()?;
    let name = target.file_name()?.to_str()?; // e.g. "nvme0"
    name.strip_prefix("nvme")?.parse().ok()
}

fn read_model(index: u32) -> Option<String> {
    if index == u32::MAX {
        return None;
    }
    let path = format!("/sys/class/nvme/nvme{index}/model");
    let mut buffer = [0; hwmon::SYSFS_BUFFER_SIZE];
    Some(hwmon::read_sysfs(Path::new(&path), &mut buffer)?.to_owned())
}
