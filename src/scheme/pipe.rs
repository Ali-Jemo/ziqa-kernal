/// Pipe Scheme for ZiqaKernel
/// Inspired by Redox OS, simplified for our capability model.

use alloc::collections::BTreeMap;
use alloc::collections::VecDeque;
use alloc::sync::Arc;
use spin::Mutex;
use core::sync::atomic::{AtomicUsize, Ordering};
use crate::scheme::{Scheme, SchemeResult};
use crate::sync::WaitCondition;

static PIPE_NEXT_ID: AtomicUsize = AtomicUsize::new(1);

pub struct Pipe {
    queue: Mutex<VecDeque<u8>>,
    read_condition: WaitCondition,
    write_condition: WaitCondition,
}

pub struct PipeScheme {
    pipes: Mutex<BTreeMap<usize, Arc<Pipe>>>,
}

impl PipeScheme {
    pub fn new() -> Self {
        Self {
            pipes: Mutex::new(BTreeMap::new()),
        }
    }

    /// Create a new pipe pair (read_id, write_id)
    /// In this simplified version, we return one ID and use flags to distinguish.
    pub fn create_pipe(&self) -> usize {
        let id = PIPE_NEXT_ID.fetch_add(1, Ordering::SeqCst);
        let pipe = Arc::new(Pipe {
            queue: Mutex::new(VecDeque::new()),
            read_condition: WaitCondition::new(),
            write_condition: WaitCondition::new(),
        });
        self.pipes.lock().insert(id, pipe);
        id
    }
}

impl Scheme for PipeScheme {
    fn open(&self, _path: &str, _flags: usize) -> SchemeResult<usize> {
        // For simplicity, opening "pipe:" creates a new pipe
        Ok(self.create_pipe())
    }

    fn read(&self, id: usize, buf: &mut [u8]) -> SchemeResult<usize> {
        let pipe = self.pipes.lock().get(&id).cloned().ok_or(crate::abi::AbiError::Other("Invalid pipe ID"))?;
        
        loop {
            let mut queue = pipe.queue.lock();
            if !queue.is_empty() {
                let mut i = 0;
                while i < buf.len() && !queue.is_empty() {
                    buf[i] = queue.pop_front().unwrap();
                    i += 1;
                }
                pipe.write_condition.notify();
                return Ok(i);
            }
            drop(queue);
            pipe.read_condition.wait("pipe_read");
        }
    }

    fn write(&self, id: usize, buf: &[u8]) -> SchemeResult<usize> {
        let pipe = self.pipes.lock().get(&id).cloned().ok_or(crate::abi::AbiError::Other("Invalid pipe ID"))?;
        
        let mut queue = pipe.queue.lock();
        // Simplified: unbounded pipe for now
        for &b in buf {
            queue.push_back(b);
        }
        pipe.read_condition.notify();
        Ok(buf.len())
    }

    fn close(&self, id: usize) -> SchemeResult<()> {
        self.pipes.lock().remove(&id);
        Ok(())
    }
}
