/// ABI Plugin Architecture for ZiqaKernel
///
/// This is the key innovation: instead of hardcoding one ABI (Linux/Windows/etc),
/// the kernel defines a generic `AbiPlugin` trait. Each supported binary format
/// registers a plugin that knows how to:
///   1. Detect if a binary belongs to it (magic bytes)
///   2. Load the binary into memory
///   3. Handle syscalls from that binary
///
/// This lets ZiqaKernel run Linux ELF binaries, WASM modules, and future
/// native formats — all through the same unified interface.

pub mod linux;
pub mod wasm;
pub mod syscall;

use crate::process::Process;

/// Result type for ABI operations
#[derive(Debug)]
pub enum AbiError {
    /// Binary format not recognized
    UnknownFormat,
    /// ELF/WASM parsing failed
    ParseError,
    /// Memory allocation failed during load
    OutOfMemory,
    /// Syscall number not implemented
    UnsupportedSyscall(u64),
    /// Permission denied by capability check
    PermissionDenied,
    /// Out of bounds access (e.g. block device or array bounds)
    OutOfBounds,
    /// Generic error with a static message
    Other(&'static str),
}

/// The core ABI Plugin trait
///
/// Any ABI implementation (Linux, WASM, etc) must implement this trait.
/// The kernel dispatches to the appropriate plugin based on the binary format.
pub trait AbiPlugin {
    /// Human-readable name of this ABI (e.g., "Linux x86_64 ELF")
    fn name(&self) -> &'static str;

    /// Which ABI kind this plugin handles
    fn kind(&self) -> crate::process::AbiKind;

    /// Check if this plugin can handle the given binary data
    /// by examining magic bytes (ELF: 0x7F 'E' 'L' 'F', WASM: 0x00 'a' 's' 'm')
    fn can_load(&self, binary: &[u8]) -> bool;

    /// Load a binary into a process, returning the configured Process
    fn load(&self, binary: &[u8], process: &mut Process) -> Result<(), AbiError>;

    /// Handle a syscall from a process running under this ABI
    ///
    /// # Arguments
    /// * `ctx` - The syscall context (number, arguments, process reference)
    ///
    /// # Returns
    /// The syscall return value (placed in RAX)
    fn handle_syscall(&self, ctx: &mut syscall::SyscallContext) -> Result<u64, AbiError>;
}

/// Maximum number of registered ABI plugins
const MAX_PLUGINS: usize = 8;

/// Registry that holds all loaded ABI plugins
pub struct AbiRegistry {
    /// Plugin slots: (kind, plugin reference)
    /// We use function pointers to static plugin instances since we're in no_std
    plugins: [Option<&'static dyn AbiPlugin>; MAX_PLUGINS],
    count: usize,
}

impl AbiRegistry {
    pub const fn new() -> Self {
        const NONE: Option<&'static dyn AbiPlugin> = None;
        Self {
            plugins: [NONE; MAX_PLUGINS],
            count: 0,
        }
    }

    /// Register a new ABI plugin
    pub fn register(&mut self, plugin: &'static dyn AbiPlugin) -> Result<(), AbiError> {
        if self.count >= MAX_PLUGINS {
            return Err(AbiError::Other("ABI registry full"));
        }
        for slot in self.plugins.iter_mut() {
            if slot.is_none() {
                *slot = Some(plugin);
                self.count += 1;
                return Ok(());
            }
        }
        Err(AbiError::Other("ABI registry full"))
    }

    /// Detect which ABI plugin can handle a binary blob
    pub fn detect(&self, binary: &[u8]) -> Option<&'static dyn AbiPlugin> {
        for slot in self.plugins.iter() {
            if let Some(plugin) = slot {
                if plugin.can_load(binary) {
                    return Some(*plugin);
                }
            }
        }
        None
    }

    /// Look up the plugin for a specific ABI kind
    pub fn get(&self, kind: crate::process::AbiKind) -> Option<&'static dyn AbiPlugin> {
        for slot in self.plugins.iter() {
            if let Some(plugin) = slot {
                if plugin.kind() == kind {
                    return Some(*plugin);
                }
            }
        }
        None
    }

    /// How many plugins are registered
    pub fn count(&self) -> usize {
        self.count
    }
}
