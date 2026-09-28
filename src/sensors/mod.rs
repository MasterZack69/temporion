//! Sensor discovery and sampling.

mod cpu;
mod disk;
mod gpu;
mod hwmon;

use std::fmt::Write as _;

use cpu::Cpu;
use disk::Disk;
use gpu::Gpu;

/// Convert a hwmon millidegree reading to rounded whole degrees Celsius.
pub(crate) fn milli_to_deg(milli: i32) -> i32 {
    (milli + 500) / 1000
}

/// Append a temperature field to `out`: the integer, or `-` when absent.
fn push_field(out: &mut String, value: Option<i32>) {
    match value {
        Some(v) => {
            let _ = write!(out, "{v}");
        }
        None => out.push('-'),
    }
}

pub struct Sensors {
    cpu: Option<Cpu>,
    gpu: Gpu,
    disks: Vec<Disk>,
}

impl Sensors {
    /// Discover sensor paths once at startup; the hot path only reads them.
    pub fn discover(gpu_always: bool) -> Self {
        Sensors {
            cpu: Cpu::discover(),
            gpu: Gpu::discover(gpu_always),
            disks: Disk::discover_all(),
        }
    }

    /// Write one sample line (no trailing newline) into `out`.
    pub fn sample(&mut self, out: &mut String) {
        push_field(out, self.cpu.as_ref().and_then(Cpu::read));
        out.push(' ');
        push_field(out, self.gpu.read());
        for disk in &self.disks {
            out.push(' ');
            push_field(out, disk.read());
        }
    }

    /// Print what was discovered to stderr (once), so the layout is verifiable.
    pub fn log_summary(&self) {
        eprintln!(
            "temporiond: cpu={}, gpu={}, disks={}",
            if self.cpu.is_some() {
                "k10temp/Tctl"
            } else {
                "not found"
            },
            self.gpu.summary(),
            self.disks.len(),
        );
        for (i, d) in self.disks.iter().enumerate() {
            eprintln!("temporiond:   disk{i} = {} ({})", d.node(), d.model());
        }
    }
}
