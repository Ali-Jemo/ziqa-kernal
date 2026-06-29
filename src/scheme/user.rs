use crate::scheme::{Scheme, SchemeResult};
use crate::sync::WaitQueue;

#[derive(Debug, Clone, Copy)]
#[repr(usize)]
pub enum PacketKind {
    Open = 1,
    Read = 2,
    Write = 3,
    Close = 4,
}

#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct Packet {
    pub id: u64,
    pub kind: usize,
    pub arg1: usize,
    pub arg2: usize,
    pub arg3: usize,
}

#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct Response {
    pub id: u64,
    pub status: isize,
}

pub struct UserScheme {
    pub todo: WaitQueue<Packet>,
    pub responses: spin::Mutex<alloc::collections::BTreeMap<u64, (alloc::sync::Arc<crate::sync::WaitCondition>, Option<isize>)>>,
    pub next_id: core::sync::atomic::AtomicU64,
}

impl UserScheme {
    pub fn new() -> Self {
        Self {
            todo: WaitQueue::new(),
            responses: spin::Mutex::new(alloc::collections::BTreeMap::new()),
            next_id: core::sync::atomic::AtomicU64::new(1),
        }
    }

    pub fn call(&self, kind: PacketKind, arg1: usize, arg2: usize, arg3: usize) -> SchemeResult<usize> {
        let id = self.next_id.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        let cond = alloc::sync::Arc::new(crate::sync::WaitCondition::new());
        
        self.responses.lock().insert(id, (cond.clone(), None));
        
        self.todo.send(Packet {
            id,
            kind: kind as usize,
            arg1,
            arg2,
            arg3,
        });
        
        cond.wait("userscheme_call");
        
        let status = self.responses.lock().remove(&id).unwrap().1.unwrap();
        
        if status < 0 {
            Err(crate::abi::AbiError::Other("Userspace daemon returned error"))
        } else {
            Ok(status as usize)
        }
    }
    
    pub fn respond(&self, resp: Response) {
        let mut resps = self.responses.lock();
        if let Some(entry) = resps.get_mut(&resp.id) {
            entry.1 = Some(resp.status);
            entry.0.notify();
        }
    }
}

impl Scheme for UserScheme {
    fn open(&self, path: &str, _flags: usize) -> SchemeResult<usize> {
        if path.starts_with(':') {
            return Ok(usize::MAX);
        }
        self.call(PacketKind::Open, 0, 0, 0)
    }

    fn read(&self, id: usize, buf: &mut [u8]) -> SchemeResult<usize> {
        if id == usize::MAX {
            if buf.len() < core::mem::size_of::<Packet>() {
                return Err(crate::abi::AbiError::Other("Buffer too small for packet"));
            }
            if let Some(packet) = self.todo.receive_nonblocking() {
                unsafe {
                    core::ptr::copy_nonoverlapping(
                        &packet as *const _ as *const u8,
                        buf.as_mut_ptr(),
                        core::mem::size_of::<Packet>()
                    );
                }
                Ok(core::mem::size_of::<Packet>())
            } else {
                Err(crate::abi::AbiError::Other("EAGAIN"))
            }
        } else {
            self.call(PacketKind::Read, id, buf.as_mut_ptr() as usize, buf.len())
        }
    }

    fn write(&self, id: usize, buf: &[u8]) -> SchemeResult<usize> {
        if id == usize::MAX {
            if buf.len() < core::mem::size_of::<Response>() {
                return Err(crate::abi::AbiError::Other("Buffer too small for response"));
            }
            let resp = unsafe { *(buf.as_ptr() as *const Response) };
            self.respond(resp);
            return Ok(core::mem::size_of::<Response>());
        }
        self.call(PacketKind::Write, id, buf.as_ptr() as usize, buf.len())
    }

    fn close(&self, id: usize) -> SchemeResult<()> {
        if id == usize::MAX {
            return Ok(());
        }
        self.call(PacketKind::Close, id, 0, 0).map(|_| ())
    }
    fn fevent(&self, id: usize, flags: usize) -> SchemeResult<usize> {
        if id == usize::MAX && !self.todo.is_empty() {
            Ok(flags & 1)
        } else {
            Ok(0)
        }
    }
}
