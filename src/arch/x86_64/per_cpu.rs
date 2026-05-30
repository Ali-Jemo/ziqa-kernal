use core::sync::atomic::{AtomicU64, Ordering};
use x86_64::registers::model_specific::Msr;

pub const MAX_CPUS: usize = 64;

#[repr(C, align(64))]
pub struct PerCpu {
    /// Index of this CPU (0 = BSP)
    pub cpu_id: u32,
    /// Local APIC ID from MADT
    pub apic_id: u32,
    /// Whether this CPU is the BSP
    pub is_bsp: bool,
    _pad: [u8; 7],
    /// PID of the currently running process on this CPU (u64::MAX = none/idle)
    pub current_pid: AtomicU64,
    /// Top of kernel stack for this CPU
    pub kernel_stack_top: u64,
    /// Saved kernel stack pointer for context switching
    pub kernel_stack_ptr: u64,
    /// Physical frame of this CPU's page table (Cr3 value)
    pub cr3: u64,
    /// Number of timer ticks on this CPU
    pub ticks: AtomicU64,
    pub tlb_shootdown_in_progress: bool,
    _pad2: [u8; 7],
    pub tlb_shootdown_addr: u64,
}

impl PerCpu {
    pub const fn empty() -> Self {
        Self {
            cpu_id: 0,
            apic_id: 0,
            is_bsp: false,
            _pad: [0; 7],
            current_pid: AtomicU64::new(u64::MAX),
            kernel_stack_top: 0,
            kernel_stack_ptr: 0,
            cr3: 0,
            ticks: AtomicU64::new(0),
            tlb_shootdown_in_progress: false,
            _pad2: [0; 7],
            tlb_shootdown_addr: 0,
        }
    }

    pub fn current_pid(&self) -> Option<crate::process::Pid> {
        let val = self.current_pid.load(Ordering::Relaxed);
        if val == u64::MAX {
            None
        } else {
            Some(crate::process::Pid(val))
        }
    }

    pub fn set_current_pid(&self, pid: Option<crate::process::Pid>) {
        let val = pid.map_or(u64::MAX, |p| p.0);
        self.current_pid.store(val, Ordering::Relaxed);
    }

    pub fn tick(&self) {
        self.ticks.fetch_add(1, Ordering::Relaxed);
    }

    pub fn total_ticks(&self) -> u64 {
        self.ticks.load(Ordering::Relaxed)
    }
}

/// BSP per-CPU area — statically allocated
static mut BSP_PER_CPU: PerCpu = PerCpu::empty();

/// Pointers to all per-CPU areas (indexed by cpu_id)
static mut PER_CPU_PTRS: [*const PerCpu; MAX_CPUS] = [core::ptr::null(); MAX_CPUS];

pub static CPU_COUNT: core::sync::atomic::AtomicU32 =
    core::sync::atomic::AtomicU32::new(1);

/// Initialize per-CPU data for the BSP.
/// Called very early in `init()`, before anything else.
pub fn init_bsp(apic_id: u32) {
    unsafe {
        let cpu: *mut PerCpu = &raw mut BSP_PER_CPU;
        (*cpu).cpu_id = 0;
        (*cpu).apic_id = apic_id;
        (*cpu).is_bsp = true;
        (*cpu).current_pid = AtomicU64::new(u64::MAX);
        (*cpu).ticks = AtomicU64::new(0);
        PER_CPU_PTRS[0] = cpu as *const PerCpu;
        set_gs_base(cpu as *const PerCpu as u64);
    }
}

/// Allocate a PerCpu for an AP (cpu_id > 0).
pub fn alloc_ap(apic_id: u32, cpu_id: u32) -> &'static mut PerCpu {
    let ptr = crate::memory::allocate_percpu_area();
    unsafe {
        let cpu = &mut *(ptr as *mut PerCpu);
        *cpu = PerCpu::empty();
        cpu.cpu_id = cpu_id;
        cpu.apic_id = apic_id;
        cpu.is_bsp = false;
        PER_CPU_PTRS[cpu_id as usize] = cpu as *const PerCpu;
        CPU_COUNT.store(cpu_id + 1, core::sync::atomic::Ordering::Release);
        cpu
    }
}

pub fn set_gs_base(addr: u64) {
    unsafe {
        let mut msr = Msr::new(0xC0000101); // GS.base
        msr.write(addr);
    }
}

pub fn current_cpu() -> &'static PerCpu {
    let ptr = read_gs_base();
    unsafe { &*(ptr as *const PerCpu) }
}

pub fn current_cpu_mut() -> &'static mut PerCpu {
    let ptr = read_gs_base();
    unsafe { &mut *(ptr as *mut PerCpu) }
}

fn read_gs_base() -> u64 {
    let hi: u32;
    let lo: u32;
    unsafe {
        core::arch::asm!(
            "rdmsr",
            in("ecx") 0xC0000101u32,
            out("eax") lo,
            out("edx") hi,
            options(nostack, preserves_flags),
        );
    }
    (hi as u64) << 32 | lo as u64
}

pub fn per_cpu_by_id(id: u32) -> Option<&'static PerCpu> {
    if (id as usize) < MAX_CPUS {
        unsafe {
            let ptr = PER_CPU_PTRS[id as usize];
            if !ptr.is_null() {
                return Some(&*ptr);
            }
        }
    }
    None
}

pub fn per_cpu_by_id_mut(id: u32) -> Option<&'static mut PerCpu> {
    if (id as usize) < MAX_CPUS {
        unsafe {
            let ptr = PER_CPU_PTRS[id as usize] as *mut PerCpu;
            if !ptr.is_null() {
                return Some(&mut *ptr);
            }
        }
    }
    None
}

pub fn cpu_count() -> u32 {
    CPU_COUNT.load(core::sync::atomic::Ordering::Acquire)
}
