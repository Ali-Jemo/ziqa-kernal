//! `namespace:` scheme — virtual directory listing registered desktop services.
//!
//! Provides a hierarchical view of known desktop applications/services:
//! - `open("")` or `open("/")` — lists registered service names
//! - `open("/<name>")` — shows service info (path + scheme-creation-cap)
//! - `open("/<name>/path")` — returns mount path for the binary
//! - `open("/<name>/scheme-creation-cap")` — write to register a new scheme

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::string::ToString;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicUsize, Ordering};
use spin::Mutex;

use crate::abi::AbiError;
use crate::scheme::{Scheme, SchemeResult};

/// Entry for a registered desktop service.
#[derive(Clone, Debug)]
pub struct AppEntry {
    /// Filesystem path to the binary (e.g. "/bin/terminal")
    pub path: String,
    /// Optional scheme name the service registers (e.g. Some("orbital"))
    pub scheme: Option<String>,
}

/// Static registry of known desktop applications.
static REGISTERED_APPS: Mutex<BTreeMap<String, AppEntry>> = Mutex::new(BTreeMap::new());

/// Initialize the namespace with default entries.
pub fn init() {
    let mut apps = REGISTERED_APPS.lock();
    apps.insert("orbital".into(), AppEntry {
        path: "/bin/orbital".into(),
        scheme: Some("orbital".into()),
    });
    apps.insert("input".into(), AppEntry {
        path: "".into(),
        scheme: Some("input".into()),
    });
    apps.insert("terminal".into(), AppEntry {
        path: "/bin/terminal".into(),
        scheme: None,
    });
    apps.insert("orblauncher".into(), AppEntry {
        path: "/bin/orblauncher".into(),
        scheme: None,
    });
    apps.insert("file-manager".into(), AppEntry {
        path: "/bin/file-manager".into(),
        scheme: None,
    });
}

pub struct NamespaceScheme {
    next_handle: AtomicUsize,
    /// Per-handle state: (data, offset) or special types
    handles: Mutex<BTreeMap<usize, NamespaceHandle>>,
}

enum NamespaceHandle {
    /// Listing of all services (for bare "/" open)
    List {
        data: Vec<u8>,
        offset: usize,
    },
    /// Single service info (for "/<name>" open)
    ServiceInfo {
        data: Vec<u8>,
        offset: usize,
    },
    /// Service path file (for "/<name>/path" open) — pre-computed text output
    ServicePath {
        data: Vec<u8>,
        offset: usize,
    },
    /// Scheme-creation capability handle — writes register new schemes
    SchemeCreation {
        name: String,
        buf: Vec<u8>,
    },
}

impl NamespaceScheme {
    pub const fn new() -> Self {
        Self {
            next_handle: AtomicUsize::new(1),
            handles: Mutex::new(BTreeMap::new()),
        }
    }

    fn parse_segments(path: &str) -> Vec<&str> {
        let trimmed = path.trim_start_matches('/').trim_end_matches('/');
        if trimmed.is_empty() {
            Vec::new()
        } else {
            trimmed.split('/').filter(|s| !s.is_empty()).collect()
        }
    }

    fn build_service_list() -> Vec<u8> {
        let apps = REGISTERED_APPS.lock();
        let mut s = String::new();
        // Include all registered schemes from the kernel registry
        let scheme_names = crate::scheme::SCHEME_REGISTRY.lock().iter_names();
        for name in &scheme_names {
            s.push_str(name);
            s.push('\n');
        }
        // Also list hardcoded apps that aren't kernel schemes
        for name in apps.keys() {
            if !scheme_names.contains(name) {
                s.push_str(name);
                s.push('\n');
            }
        }
        s.into_bytes()
    }

    fn build_service_info(name: &str) -> Option<Vec<u8>> {
        let apps = REGISTERED_APPS.lock();
        let entry = apps.get(name)?;
        let mut s = String::new();
        s.push_str(&alloc::format!("path: {}\n", entry.path));
        s.push_str(&alloc::format!(
            "scheme-creation-cap: /scheme/{}\n", name
        ));
        Some(s.into_bytes())
    }
}

impl Scheme for NamespaceScheme {
    fn open(&self, path: &str, _flags: usize) -> SchemeResult<usize> {
        let segments = Self::parse_segments(path);
        let handle = self.next_handle.fetch_add(1, Ordering::SeqCst);

        let h = match segments.len() {
            // open("") or open("/") — list all services
            0 => NamespaceHandle::List {
                data: Self::build_service_list(),
                offset: 0,
            },
            // open("/<name>") — service info
            1 => {
                let name = segments[0];
                match Self::build_service_info(name) {
                    Some(data) => NamespaceHandle::ServiceInfo { data, offset: 0 },
                    None => return Err(AbiError::Other("No such file or directory")),
                }
            }
            // open("/<name>/path") or open("/<name>/scheme-creation-cap")
            2 => {
                let name = segments[0];
                let attr = segments[1];
                let apps = REGISTERED_APPS.lock();
                let entry = apps.get(name).ok_or(AbiError::Other("No such service"))?;

                match attr {
                    "path" => {
                        let data = alloc::format!("{}\n[not mounted]\n", entry.path).into_bytes();
                        NamespaceHandle::ServicePath { data, offset: 0 }
                    }
                    "scheme-creation-cap" => {
                        NamespaceHandle::SchemeCreation {
                            name: name.to_string(),
                            buf: Vec::new(),
                        }
                    }
                    _ => return Err(AbiError::Other("No such file or directory")),
                }
            }
            _ => return Err(AbiError::Other("No such file or directory")),
        };

        self.handles.lock().insert(handle, h);
        Ok(handle)
    }

    fn read(&self, id: usize, buf: &mut [u8]) -> SchemeResult<usize> {
        let mut handles = self.handles.lock();
        // Borrow with &mut to use match ergonomics
        let handle = handles.get_mut(&id).ok_or(AbiError::Other("Bad file descriptor"))?;

        match handle {
            NamespaceHandle::List { data, offset }
            | NamespaceHandle::ServiceInfo { data, offset }
            | NamespaceHandle::ServicePath { data, offset } => {
                if *offset >= data.len() {
                    return Ok(0);
                }
                let remaining = data.len() - *offset;
                let to_copy = core::cmp::min(remaining, buf.len());
                buf[..to_copy].copy_from_slice(&data[*offset..*offset + to_copy]);
                *offset += to_copy;
                Ok(to_copy)
            }
            NamespaceHandle::SchemeCreation { buf: data, .. } => {
                if data.is_empty() {
                    return Ok(0);
                }
                let to_copy = core::cmp::min(data.len(), buf.len());
                buf[..to_copy].copy_from_slice(&data[..to_copy]);
                data.drain(..to_copy);
                Ok(to_copy)
            }
        }
    }

    fn write(&self, id: usize, buf: &[u8]) -> SchemeResult<usize> {
        let mut handles = self.handles.lock();
        let handle = handles.get_mut(&id).ok_or(AbiError::Other("Bad file descriptor"))?;

        match handle {
            NamespaceHandle::SchemeCreation { name, buf: storage } => {
                // Parse buffer as "name\0path" to register a new scheme
                if let Some(null_pos) = buf.iter().position(|&b| b == 0) {
                    let cap_name = core::str::from_utf8(&buf[..null_pos])
                        .map_err(|_| AbiError::Other("Invalid UTF-8"))?;
                    let cap_path = core::str::from_utf8(&buf[null_pos + 1..])
                        .map_err(|_| AbiError::Other("Invalid UTF-8"))?;

                    let mut apps = REGISTERED_APPS.lock();
                    apps.insert(cap_name.to_string(), AppEntry {
                        path: cap_path.to_string(),
                        scheme: None,
                    });
                    crate::println!(
                        "[namespace] registered scheme '{}' for service '{}'",
                        cap_name, name
                    );
                    storage.extend_from_slice(buf);
                } else {
                    storage.extend_from_slice(buf);
                }
                Ok(buf.len())
            }
            _ => Err(AbiError::Other("Read only")),
        }
    }

    fn close(&self, id: usize) -> SchemeResult<()> {
        self.handles.lock().remove(&id);
        Ok(())
    }
}
