/// x86 (32-bit) architecture support for ZiqaKernel
/// Ported and adapted from Redox OS.

pub mod interrupt;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct CpuState {
    pub ebp: u32,
    pub esi: u32,
    pub edi: u32,
    pub ebx: u32,
    pub eflags: u32,
    pub esp: u32,
    pub fsbase: u32,
    pub gsbase: u32,
    pub eip: u32,
}

impl CpuState {
    pub const fn zero() -> Self {
        Self {
            ebp: 0, esi: 0, edi: 0, ebx: 0,
            eflags: 0, esp: 0, fsbase: 0, gsbase: 0, eip: 0,
        }
    }
}

pub fn new_user_state(entry: crate::memory::VirtAddr, stack: crate::memory::VirtAddr) -> CpuState {
    CpuState {
        eip: entry.as_u64() as u32,
        esp: stack.as_u64() as u32,
        eflags: 0x202, // IF set
        ..CpuState::zero()
    }
}

pub const KERNEL_CODE_SEL: u16 = 0x08;
pub const KERNEL_DATA_SEL: u16 = 0x10;
pub const USER_CODE_SEL: u16 = 0x1B;
pub const USER_DATA_SEL: u16 = 0x23;
pub const TSS_SEL: u16 = 0x28;

#[unsafe(naked)]
pub unsafe extern "C" fn switch_context(old_sp: *mut u32, new_sp: u32) {
    core::arch::naked_asm!(
        "push ebp",
        "push esi",
        "push edi",
        "push ebx",
        "mov [eax], esp",
        "mov esp, edx",
        "pop ebx",
        "pop edi",
        "pop esi",
        "pop ebp",
        "ret"
    );
}

pub fn init_kthread_stack(proc: &mut crate::process::Process, entry: u64, arg: u64) {
    if let Some(ref mut _kstack) = proc.kernel_stack {
        let kstack_top = proc.kernel_stack_top;
        unsafe {
            let mut sp = kstack_top as u32;
            sp -= 4; *(sp as *mut u32) = proc.entry_point.as_u64() as u32;
            sp -= 4; *(sp as *mut u32) = 0;
            sp -= 4; *(sp as *mut u32) = 0;
            sp -= 4; *(sp as *mut u32) = arg as u32;
            proc.kernel_stack_ptr = sp as u64;
        }
    }
}
