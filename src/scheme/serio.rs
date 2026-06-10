use spin::Mutex;
use crate::scheme::{Scheme, SchemeResult};
use crate::abi::AbiError;
use crate::sync::wait_queue::WaitQueue;
/// Maximum number of PS/2 devices (keyboard + mouse)
const SERIO_DEVICES: usize = 2;

/// Input queue for each device (keyboard=0, mouse=1)
static INPUT_QUEUES: [WaitQueue<u8>; SERIO_DEVICES] = [
    WaitQueue::new(),
    WaitQueue::new(),
];

/// Maximum queue size per device
const MAX_QUEUE_SIZE: usize = 256;

#[derive(Clone, Copy, PartialEq, Eq)]
enum HandleKind {
    Device(usize),
    SchemeRoot,
}

#[derive(Clone, Copy)]
struct Handle {
    kind: HandleKind,
}

/// Serio scheme handle map
type HandleMap = alloc::collections::BTreeMap<usize, Handle>;

static HANDLES: Mutex<HandleMap> = Mutex::new(alloc::collections::BTreeMap::new());
static NEXT_HANDLE: Mutex<usize> = Mutex::new(1);

pub struct SerioScheme;

impl SerioScheme {
    pub const fn new() -> Self {
        Self
    }

    /// Called from interrupt handlers to push input data
    /// index: 0 = keyboard, 1 = mouse
    pub fn input(index: usize, data: u8) {
        if index >= SERIO_DEVICES {
            return;
        }

        let queue = &INPUT_QUEUES[index];
        if queue.len() < MAX_QUEUE_SIZE {
            queue.send(data);
        }
    }
}

impl Scheme for SerioScheme {
    fn open(&self, path: &str, _flags: usize) -> SchemeResult<usize> {
        let kind = if path.is_empty() || path == "/" {
            HandleKind::SchemeRoot
        } else if let Ok(idx) = path.parse::<usize>() {
            if idx < SERIO_DEVICES {
                HandleKind::Device(idx)
            } else {
                return Err(AbiError::Other("InvalidPath"));
            }
        } else {
            return Err(AbiError::Other("InvalidPath"));
        };

        let mut handles = HANDLES.lock();
        let mut next_handle = NEXT_HANDLE.lock();
        let handle = *next_handle;
        *next_handle += 1;
        handles.insert(handle, Handle { kind });
        Ok(handle)
    }

    fn read(&self, id: usize, buf: &mut [u8]) -> SchemeResult<usize> {
        let handle = {
            let handles = HANDLES.lock();
            handles.get(&id).copied().ok_or(AbiError::Other("BadFileDescriptor"))?
        };

        let HandleKind::Device(index) = handle.kind else {
            return Err(AbiError::Other("InvalidPath"));
        };

        if buf.is_empty() {
            return Ok(0);
        }

        let queue = &INPUT_QUEUES[index];
        let mut bytes_read = 0;

        // Non-blocking read: return what's available
        while bytes_read < buf.len() {
            if let Some(byte) = queue.receive_nonblocking() {
                buf[bytes_read] = byte;
                bytes_read += 1;
            } else {
                break;
            }
        }

        if bytes_read == 0 {
            // No data available - return EAGAIN equivalent
            return Err(AbiError::Other("Again"));
        }

        Ok(bytes_read)
    }

    fn write(&self, id: usize, buf: &[u8]) -> SchemeResult<usize> {
        let handle = {
            let handles = HANDLES.lock();
            handles.get(&id).copied().ok_or(AbiError::Other("BadFileDescriptor"))?
        };

        match handle.kind {
            HandleKind::Device(index) => {
                // Write to device (e.g., send commands to keyboard/mouse)
                // For now, just delegate to existing drivers
                for &byte in buf {
                    if index == 0 {
                        // Keyboard: push raw byte for userspace driver
                        crate::drivers::keyboard::push_raw_byte(byte);
                    } else if index == 1 {
                        // Mouse: send command byte
                        crate::drivers::ps2_mouse::mouse_write(byte);
                    }
                }
                Ok(buf.len())
            }
            HandleKind::SchemeRoot => Err(AbiError::Other("InvalidPath")),
        }
    }

    fn close(&self, id: usize) -> SchemeResult<()> {
        let mut handles = HANDLES.lock();
        handles.remove(&id);
        Ok(())
    }
}

/// Initialize the serio scheme and register it
pub fn init() {
    let mut registry = crate::scheme::SCHEME_REGISTRY.lock();
    registry.register("serio", alloc::boxed::Box::new(SerioScheme::new()));
}
