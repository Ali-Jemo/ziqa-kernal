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

use self::debug::DebugScheme;
use self::pipe::PipeScheme;
use self::time::TimeScheme;
use self::proc::ProcScheme;

use self::irq::IrqSchemeWrapper;
use self::keyboard::KeyboardScheme;

pub type SchemeResult<T> = Result<T, crate::abi::AbiError>;

pub trait Scheme: Send + Sync {
    /// Open a resource at the given path
    fn open(&self, path: &str, flags: usize) -> SchemeResult<usize>;
    
    /// Read from a resource
    fn read(&self, id: usize, buf: &mut [u8]) -> SchemeResult<usize>;
    
    /// Write to a resource
    fn write(&self, id: usize, buf: &[u8]) -> SchemeResult<usize>;
    
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
}
