//! Small shared helpers for reading Linux hwmon sysfs nodes.

use std::fs;
use std::path::{Path, PathBuf};

/// Read a `tempN_input` node (millidegrees C) as an integer.
pub fn read_millideg(path: &Path) -> Option<i32> {
    let raw = fs::read_to_string(path).ok()?;
    raw.trim().parse().ok()
}

/// Read a hwmon device `name` (e.g. "k10temp", "nvme"), trimmed.
pub fn device_name(dir: &Path) -> Option<String> {
    let raw = fs::read_to_string(dir.join("name")).ok()?;
    Some(raw.trim().to_owned())
}

/// Within a hwmon directory, find the `tempN_input` whose `tempN_label`
/// matches `label` exactly. Returns the input path if it exists.
pub fn labeled_input(dir: &Path, label: &str) -> Option<PathBuf> {
    for entry in fs::read_dir(dir).ok()?.flatten() {
        let file = entry.file_name();
        let file = file.to_str()?;
        let idx = match file
            .strip_prefix("temp")
            .and_then(|r| r.strip_suffix("_label"))
        {
            Some(idx) => idx,
            None => continue,
        };
        let value = fs::read_to_string(entry.path()).unwrap_or_default();
        if value.trim() == label {
            let input = dir.join(format!("temp{idx}_input"));
            if input.exists() {
                return Some(input);
            }
        }
    }
    None
}
