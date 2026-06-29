use alloc::sync::Arc;
use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use spin::Mutex;
use crate::scheme::{Scheme, SchemeResult};
use crate::abi::AbiError;
use crate::capability::ResourceKind;
use crate::sync::WaitCondition;

pub struct IrqState {
    pub condition: Arc<WaitCondition>,
    pub queue: Vec<u8>,
}

pub struct IrqScheme {
    pub irqs: Mutex<BTreeMap<u8, IrqState>>,
}

impl IrqScheme {
    pub fn open(&self, path: &str, _flags: usize) -> SchemeResult<usize> {
        // Check capability
        let has_cap = crate::process::scheduler::with_current_task(|proc| {
            proc.capabilities.has_permission(ResourceKind::DeviceIo, false, false)
        }).unwrap_or(false);

        if !has_cap {
            return Err(AbiError::PermissionDenied);
        }

        // Parse IRQ number
        let irq_num = if let Some(pos) = path.find(':') {
            path[pos + 1..].parse::<u8>().map_err(|_| AbiError::Other("Invalid argument"))?
        } else {
            return Err(AbiError::Other("Invalid argument"));
        };

        // Translate IRQ 1 (Keyboard) to vector 33 and IRQ 12 (Mouse) to vector 44
        let mut vector_num = irq_num;
        if irq_num == 1 {
            vector_num = 33;
        } else if irq_num == 12 {
            vector_num = 44;
        }

        // Initialize state for this IRQ if not already present
        let mut irqs = self.irqs.lock();
        if !irqs.contains_key(&vector_num) {
            irqs.insert(vector_num, IrqState {
                condition: Arc::new(WaitCondition::new()),
                queue: Vec::new(),
            });
        }

        // Return the IRQ number as the handle ID
        Ok(irq_num as usize)
    }

    pub fn read(&self, id: usize, buf: &mut [u8]) -> SchemeResult<usize> {
        let irq_num = id as u8;
        let mut vector_num = irq_num;
        if irq_num == 1 {
            vector_num = 33;
        } else if irq_num == 12 {
            vector_num = 44;
        }

        let cond = {
            let irqs = self.irqs.lock();
            if let Some(state) = irqs.get(&vector_num) {
                state.condition.clone()
            } else {
                return Err(AbiError::Other("Invalid argument"));
            }
        };

        // Wait for the interrupt to fire
        cond.wait("irq");

        // After waking up, return data from queue
        let mut irqs = self.irqs.lock();
        if let Some(state) = irqs.get_mut(&vector_num) {
            let len = core::cmp::min(buf.len(), state.queue.len());
            let data: Vec<u8> = state.queue.drain(..len).collect();
            buf[..len].copy_from_slice(&data);
            Ok(len)
        } else {
            Err(AbiError::Other("Invalid argument"))
        }
    }

    pub fn write(&self, _id: usize, _buf: &[u8]) -> SchemeResult<usize> {
        Err(AbiError::Other("Not implemented"))
    }

    pub fn close(&self, _id: usize) -> SchemeResult<()> {
        Ok(())
    }
}

lazy_static::lazy_static! {
    pub static ref IRQ_SCHEME: IrqScheme = IrqScheme {
        irqs: Mutex::new(BTreeMap::new()),
    };
}

pub fn irq_trigger(vector: u8, data: &[u8]) {
    let mut irqs = IRQ_SCHEME.irqs.lock();
    if let Some(state) = irqs.get_mut(&vector) {
        state.queue.extend_from_slice(data);
        state.condition.notify();
    }
}

pub struct IrqSchemeWrapper;

impl IrqSchemeWrapper {
    pub const fn new() -> Self {
        Self
    }
}

impl Scheme for IrqSchemeWrapper {
    fn open(&self, path: &str, flags: usize) -> SchemeResult<usize> {
        IRQ_SCHEME.open(path, flags)
    }

    fn read(&self, id: usize, buf: &mut [u8]) -> SchemeResult<usize> {
        IRQ_SCHEME.read(id, buf)
    }

    fn write(&self, id: usize, buf: &[u8]) -> SchemeResult<usize> {
        IRQ_SCHEME.write(id, buf)
    }

    fn close(&self, id: usize) -> SchemeResult<()> {
        IRQ_SCHEME.close(id)
    }
}
