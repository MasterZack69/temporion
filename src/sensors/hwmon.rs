//! Small shared helpers for reading Linux hwmon sysfs nodes.

use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

/// Enough for the small sysfs attributes used here (values, names and labels).
pub const SYSFS_BUFFER_SIZE: usize = 256;

/// Read and trim a sysfs attribute without allocating a temporary String.
/// Reject invalid UTF-8 and oversized values rather than parsing a truncated read.
pub fn read_sysfs<'a>(path: &Path, buffer: &'a mut [u8]) -> Option<&'a str> {
    read_sysfs_from(fs::File::open(path).ok()?, buffer)
}

fn read_sysfs_from(mut reader: impl Read, buffer: &mut [u8]) -> Option<&str> {
    let mut used = 0;
    loop {
        // Probe for EOF when full: even an exact-fit attribute must be accepted,
        // but a longer one must never be silently truncated.
        let mut extra = [0; 1];
        let target = if used == buffer.len() {
            &mut extra[..]
        } else {
            &mut buffer[used..]
        };
        match reader.read(target) {
            Ok(0) => return std::str::from_utf8(&buffer[..used]).ok().map(str::trim),
            Ok(_) if used == buffer.len() => return None,
            Ok(n) => used += n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => return None,
        }
    }
}

/// Read a `tempN_input` node (millidegrees C) as an integer.
pub fn read_millideg(path: &Path) -> Option<i32> {
    let mut buffer = [0; SYSFS_BUFFER_SIZE];
    read_sysfs(path, &mut buffer)?.parse().ok()
}

/// Read a hwmon device `name` (e.g. "k10temp", "nvme"), trimmed.
pub fn device_name(dir: &Path) -> Option<String> {
    let mut buffer = [0; SYSFS_BUFFER_SIZE];
    Some(read_sysfs(&dir.join("name"), &mut buffer)?.to_owned())
}

/// Within a hwmon directory, find the `tempN_input` whose `tempN_label`
/// matches `label` exactly. Returns the input path if it exists.
pub fn labeled_input(dir: &Path, label: &str) -> Option<PathBuf> {
    let mut buffer = [0; SYSFS_BUFFER_SIZE];
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
        if read_sysfs(&entry.path(), &mut buffer).unwrap_or_default() == label {
            let input = dir.join(format!("temp{idx}_input"));
            if input.exists() {
                return Some(input);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn trims_and_parses_temperature_values() {
        let mut buffer = [0; SYSFS_BUFFER_SIZE];
        for (raw, expected) in [("61000\n", 61000), (" -12500 \n", -12500)] {
            let text = read_sysfs_from(Cursor::new(raw), &mut buffer).unwrap();
            assert_eq!(text.parse::<i32>().ok(), Some(expected));
        }
        for raw in ["", "\n", "invalid\n", "2147483648\n"] {
            let text = read_sysfs_from(Cursor::new(raw), &mut buffer).unwrap();
            assert!(text.parse::<i32>().is_err());
        }
    }

    #[test]
    fn reads_status_and_reuses_only_initialized_bytes() {
        let mut buffer = [0; SYSFS_BUFFER_SIZE];
        assert_eq!(
            read_sysfs_from(Cursor::new("suspended\n"), &mut buffer),
            Some("suspended")
        );
        assert_eq!(
            read_sysfs_from(Cursor::new("active\n"), &mut buffer),
            Some("active")
        );
    }

    #[test]
    fn rejects_invalid_utf8_and_oversized_input() {
        let mut buffer = [0; 4];
        assert_eq!(read_sysfs_from(Cursor::new([0xff]), &mut buffer), None);
        assert_eq!(read_sysfs_from(Cursor::new("12345"), &mut buffer), None);
        assert_eq!(
            read_sysfs_from(Cursor::new("1234"), &mut buffer),
            Some("1234")
        );
    }

    #[test]
    fn handles_short_reads_and_interrupted_reads() {
        struct ShortReader {
            inner: Cursor<&'static [u8]>,
            interrupt: bool,
        }
        impl Read for ShortReader {
            fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
                if self.interrupt {
                    self.interrupt = false;
                    return Err(io::Error::from(io::ErrorKind::Interrupted));
                }
                self.inner.read(&mut buffer[..1])
            }
        }
        let reader = ShortReader {
            inner: Cursor::new(b"61000\n"),
            interrupt: true,
        };
        let mut buffer = [0; SYSFS_BUFFER_SIZE];
        assert_eq!(read_sysfs_from(reader, &mut buffer), Some("61000"));
    }

    #[test]
    fn propagates_read_and_open_failures() {
        struct BrokenReader;
        impl Read for BrokenReader {
            fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
                Err(io::Error::from(io::ErrorKind::PermissionDenied))
            }
        }
        let mut buffer = [0; SYSFS_BUFFER_SIZE];
        assert_eq!(read_sysfs_from(BrokenReader, &mut buffer), None);
        // A NUL-containing path is invalid on Linux and cannot exist.
        assert_eq!(read_millideg(Path::new("invalid\0path")), None);
    }
}
