use crate::ipc::shm::SHM;
use crate::process::Pid;
use crate::abi::AbiError;
use spin::Mutex;

pub const SLOTS: usize = 64;
pub const SLOT_SIZE: usize = 1536;

#[repr(C)]
pub struct NetRingControl {
    pub rx_head: u32, // Produced by driver, consumed by kernel
    pub rx_tail: u32,
    pub tx_head: u32, // Produced by kernel, consumed by driver
    pub tx_tail: u32,
    pub num_slots: u32,
    pub slot_size: u32,
}

#[repr(C)]
pub struct NetSlot {
    pub len: u32,
    pub data: [u8; SLOT_SIZE],
}

#[repr(C)]
pub struct NetBridge {
    pub ctrl: NetRingControl,
    pub rx_slots: [NetSlot; SLOTS],
    pub tx_slots: [NetSlot; SLOTS],
}

pub static BRIDGE: Mutex<Option<u64>> = Mutex::new(None);

pub fn init_bridge() -> Result<u32, AbiError> {
    let size = core::mem::size_of::<NetBridge>();
    let mut shm = SHM.lock();
    let id = shm.create(Pid(0), size)?;
    let addr = shm.attach(id, Pid(0))?;
    
    // Initialize the bridge structure
    unsafe {
        let bridge = &mut *(addr as *mut NetBridge);
        bridge.ctrl.rx_head = 0;
        bridge.ctrl.rx_tail = 0;
        bridge.ctrl.tx_head = 0;
        bridge.ctrl.tx_tail = 0;
        bridge.ctrl.num_slots = SLOTS as u32;
        bridge.ctrl.slot_size = SLOT_SIZE as u32;
    }

    *BRIDGE.lock() = Some(addr);
    crate::klog!(crate::klog::Level::Info, "NET: Bridge SHM region {} created at 0x{:x}", id, addr);
    Ok(id)
}

pub fn get_bridge() -> Option<&'static mut NetBridge> {
    BRIDGE.lock().map(|addr| unsafe { &mut *(addr as *mut NetBridge) })
}
