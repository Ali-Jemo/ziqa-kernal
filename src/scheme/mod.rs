/// ZiqaScheme: Unified resource management system inspired by Redox OS.
///
/// In this system, everything is a resource identified by a URL (scheme:path).
/// Drivers, filesystems, and network protocols all implement this trait.

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::string::ToString;
use spin::Mutex;

pub mod debug;
pub mod pipe;
pub mod user;
pub mod time;
pub mod irq;
pub mod keyboard;
pub mod proc;
pub mod acpi;
pub mod dtb;
pub mod serio;
pub mod memory;
pub mod event;
pub mod sys;
pub mod orbital_bridge;

use self::debug::DebugScheme;
use self::pipe::PipeScheme;
use self::time::TimeScheme;
use self::proc::ProcScheme;
use self::irq::IrqSchemeWrapper;
use self::keyboard::KeyboardScheme;
use self::dtb::DtbScheme;
use self::serio::SerioScheme;
use self::acpi::AcpiScheme;
use self::memory::MemoryScheme;
use self::event::EventScheme;
use self::user::UserScheme;
use self::sys::SysScheme;

pub type SchemeResult<T> = Result<T, crate::abi::AbiError>;

pub trait Scheme: Send + Sync {
    /// Open a resource at the given path
    fn open(&self, path: &str, flags: usize) -> SchemeResult<usize>;
    
    /// Read from a resource
    fn read(&self, id: usize, buf: &mut [u8]) -> SchemeResult<usize>;
    
    /// Write to a resource
    fn write(&self, id: usize, buf: &[u8]) -> SchemeResult<usize>;
    
    /// Check for readiness events. Used by the event scheme.
    fn fevent(&self, _id: usize, _flags: usize) -> SchemeResult<usize> {
        Ok(0) // Default: not ready
    }
    
    /// Close a resource
    fn close(&self, id: usize) -> SchemeResult<()>;
}

pub struct SchemeRegistry {
    schemes: BTreeMap<String, Box<dyn Scheme>>,
}

impl SchemeRegistry {
    pub const fn new() -> Self {
        Self {
            schemes: BTreeMap::new(),
        }
    }
    
    pub fn iter_names(&self) -> alloc::vec::Vec<String> {
        self.schemes.keys().cloned().collect()
    }
    
    pub fn register(&mut self, name: &str, scheme: Box<dyn Scheme>) {
        self.schemes.insert(name.to_string(), scheme);
    }
    
    pub fn get(&self, name: &str) -> Option<&dyn Scheme> {
        self.schemes.get(name).map(|s| s.as_ref())
    }
}

pub static SCHEME_REGISTRY: Mutex<SchemeRegistry> = Mutex::new(SchemeRegistry::new());

pub fn init() {
    let mut registry = SCHEME_REGISTRY.lock();
    registry.register("debug", Box::new(DebugScheme::new()));
    registry.register("pipe", Box::new(PipeScheme::new()));
    registry.register("time", Box::new(TimeScheme::new()));
    registry.register("proc", Box::new(ProcScheme::new()));
    registry.register("irq", Box::new(IrqSchemeWrapper::new()));
    registry.register("keyboard", Box::new(KeyboardScheme::new()));
    registry.register("acpi", Box::new(AcpiScheme::new()));
    registry.register("dtb", Box::new(DtbScheme::new()));
    registry.register("memory", Box::new(MemoryScheme::new()));
    registry.register("event", Box::new(EventScheme::new()));
    registry.register("serio", Box::new(SerioScheme::new()));
    registry.register("user", Box::new(UserScheme::new()));
    registry.register("sys", Box::new(SysScheme::new()));
    // Log registered schemes
    crate::klog!(crate::klog::Level::Info, "init: ZiqaScheme registry initialized");
    crate::klog!(crate::klog::Level::Info, "Scheme: acpi, dtb, memory, event, serio, user, sys registered");
}