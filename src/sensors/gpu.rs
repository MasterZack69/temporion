//! NVIDIA GPU temperature via NVML (RTX 4050).
//!
//! The proprietary driver does not expose a hwmon temperature, so we call NVML
//! directly through a tiny hand-written FFI binding loaded with `dlopen`. This
//! avoids spawning `nvidia-smi` (slow, allocation-heavy, stutter-prone).
//!
//! Battery-friendliness: on Optimus laptops the dGPU is often runtime-suspended.
//! Actually querying NVML would force it back on, so by default we first check
//! the PCI device's `power/runtime_status` and report `-` while it is suspended
//! instead of waking it. Set TEMPORION_GPU_ALWAYS=1 to override.
//!
//! `dlopen`ing the library does not power on the GPU (only device queries do),
//! so we load it eagerly to detect presence, but defer `nvmlInit` + the query
//! until the GPU is actually awake.

use std::ffi::{c_char, c_int, c_uint, c_void, CString};
use std::fs;
use std::path::{Path, PathBuf};
use std::ptr;

use super::hwmon;

extern "C" {
    fn dlopen(filename: *const c_char, flag: c_int) -> *mut c_void;
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
}

const RTLD_NOW: c_int = 2;
const NVML_TEMPERATURE_GPU: c_uint = 0;
const NVML_SUCCESS: c_uint = 0;

type NvmlDevice = *mut c_void;
type FnInit = unsafe extern "C" fn() -> c_uint;
type FnGetHandle = unsafe extern "C" fn(c_uint, *mut NvmlDevice) -> c_uint;
type FnGetTemp = unsafe extern "C" fn(NvmlDevice, c_uint, *mut c_uint) -> c_uint;

/// An initialized NVML session bound to device 0.
struct Nvml {
    get_temp: FnGetTemp,
    device: NvmlDevice,
}

impl Nvml {
    /// Resolve symbols from an already-loaded library, initialize NVML and grab
    /// a handle to GPU 0. May briefly power on the GPU, so callers gate this.
    unsafe fn init(lib: *mut c_void) -> Option<Nvml> {
        let init: FnInit = std::mem::transmute(symbol(lib, "nvmlInit_v2")?);
        let get_handle: FnGetHandle =
            std::mem::transmute(symbol(lib, "nvmlDeviceGetHandleByIndex_v2")?);
        let get_temp: FnGetTemp = std::mem::transmute(symbol(lib, "nvmlDeviceGetTemperature")?);

        if init() != NVML_SUCCESS {
            return None;
        }
        let mut device: NvmlDevice = ptr::null_mut();
        if get_handle(0, &mut device) != NVML_SUCCESS {
            return None;
        }
        Some(Nvml { get_temp, device })
    }

    unsafe fn temperature(&self) -> Option<i32> {
        let mut celsius: c_uint = 0;
        if (self.get_temp)(self.device, NVML_TEMPERATURE_GPU, &mut celsius) == NVML_SUCCESS {
            Some(celsius as i32)
        } else {
            None
        }
    }
}

pub struct Gpu {
    lib: Option<*mut c_void>,
    session: Option<Nvml>,
    runtime_status: Option<PathBuf>,
    always: bool,
}

impl Gpu {
    pub fn discover(always: bool) -> Gpu {
        Gpu {
            lib: unsafe { load_library() },
            session: None,
            runtime_status: nvidia_runtime_status_path(),
            always,
        }
    }

    pub fn read(&mut self) -> Option<i32> {
        let lib = self.lib?; // no NVML library -> permanently unavailable

        // Don't wake a suspended dGPU unless explicitly told to.
        if !self.always {
            if let Some(path) = &self.runtime_status {
                let mut buffer = [0; hwmon::SYSFS_BUFFER_SIZE];
                if hwmon::read_sysfs(path, &mut buffer).unwrap_or_default() == "suspended" {
                    return None;
                }
            }
        }

        // Initialize NVML lazily, only once the GPU is actually awake.
        if self.session.is_none() {
            self.session = unsafe { Nvml::init(lib) };
        }
        unsafe { self.session.as_ref()?.temperature() }
    }

    pub fn summary(&self) -> &'static str {
        match (self.lib.is_some(), self.runtime_status.is_some()) {
            (false, _) => "unavailable",
            (true, true) => "nvml (suspend-aware)",
            (true, false) => "nvml",
        }
    }
}

/// Load libnvidia-ml, including the NixOS driver path. Does not wake the GPU.
unsafe fn load_library() -> Option<*mut c_void> {
    const CANDIDATES: [&str; 3] = [
        "libnvidia-ml.so.1",
        "libnvidia-ml.so",
        "/run/opengl-driver/lib/libnvidia-ml.so.1",
    ];
    for name in CANDIDATES {
        if let Ok(c) = CString::new(name) {
            let handle = dlopen(c.as_ptr(), RTLD_NOW);
            if !handle.is_null() {
                return Some(handle);
            }
        }
    }
    None
}

unsafe fn symbol(lib: *mut c_void, name: &str) -> Option<*mut c_void> {
    let c = CString::new(name).ok()?;
    let ptr = dlsym(lib, c.as_ptr());
    if ptr.is_null() {
        None
    } else {
        Some(ptr)
    }
}

/// Find the NVIDIA display controller's `power/runtime_status` sysfs node.
fn nvidia_runtime_status_path() -> Option<PathBuf> {
    let base = Path::new("/sys/bus/pci/devices");
    let mut buffer = [0; hwmon::SYSFS_BUFFER_SIZE];
    for entry in fs::read_dir(base).ok()?.flatten() {
        let dir = entry.path();
        if hwmon::read_sysfs(&dir.join("vendor"), &mut buffer).unwrap_or_default() != "0x10de" {
            continue; // not NVIDIA
        }
        // 0x03xxxx == display controller (VGA / 3D controller)
        if !hwmon::read_sysfs(&dir.join("class"), &mut buffer)
            .unwrap_or_default()
            .starts_with("0x03")
        {
            continue;
        }
        let status = dir.join("power/runtime_status");
        if status.exists() {
            return Some(status);
        }
    }
    None
}
