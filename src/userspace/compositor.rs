/// Native Wayland-Compatible Compositor (NWCC)
///
/// VGA-Downsampled architecture: 80×25 virtual framebuffer → physical VGA text buffer.
/// Supports: window chrome (title bar + border), taskbar, shadow, mouse drag, IPC.

use alloc::vec::Vec;
use alloc::collections::BTreeMap;
use alloc::string::String;
use crate::ipc::shm::SHM;
use crate::process::Pid;
use crate::ipc::driver::DriverClient;
use crate::ipc::driver::protocol::{DriverRequest, DriverResponse};

// ── Virtual resolution ────────────────────────────────────────────────────
// ... (rest of the file content kept)
