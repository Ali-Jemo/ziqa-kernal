use crate::abi::AbiError;
/// Shared Memory IPC for ZiqaKernel
///
/// Provides the fastest possible communication by allowing processes
/// to share the same physical memory frames. Zero-copy.
use crate::process::Pid;
use alloc::collections::BTreeMap;
use spin::Mutex;

/// A shared memory segment
pub struct ShmRegion {
    pub id: u32,
    pub owner: Pid,
    pub phys_frames: [u64; 4], // Simplified: max 16KB for now
    pub size: usize,
}

pub struct ShmManager {
    regions: BTreeMap<u32, ShmRegion>,
    next_id: u32,
}

impl ShmManager {
    pub const fn new() -> Self {
        Self {
            regions: BTreeMap::new(),
            next_id: 1,
        }
    }

    pub fn create(&mut self, owner: Pid, size: usize) -> u32 {
        let id = self.next_id;
        self.next_id += 1;

        let region = ShmRegion {
            id,
            owner,
            phys_frames: [0; 4], // In real impl, allocate from FrameAllocator
            size,
        };

        self.regions.insert(id, region);
        id
    }

    pub fn attach(&self, id: u32, _process_pid: Pid) -> Result<u64, AbiError> {
        if let Some(_region) = self.regions.get(&id) {
            // In a real impl, map phys_frames into the process's page table
            Ok(0x8000_0000) // Return virtual address where it's attached
        } else {
            Err(AbiError::Other("SHM region not found"))
        }
    }
}

pub static SHM: Mutex<ShmManager> = Mutex::new(ShmManager::new());
