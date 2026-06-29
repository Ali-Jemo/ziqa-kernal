pub mod apic;
pub mod cpu_features;
pub mod gdt;
pub mod interrupts;
pub mod per_cpu;
pub mod smp;
pub mod switch;

pub use switch::{TrapFrame, switch_context, jump_to_user_stub};
pub use per_cpu::{current_pid, set_current_pid};

// Re-export the IDT init from interrupts module for backward compat
pub mod idt {
    pub fn init_idt() {
        super::interrupts::init_idt();
    }
}

#[repr(C)]
#[repr(align(16))]
#[derive(Debug, Clone, Copy)]
pub struct FpuState {
    pub data: [u8; 512],
}

impl FpuState {
    pub fn new() -> Self {
        Self { data: [0u8; 512] }
    }
}

pub unsafe fn save_fpu(state: *mut FpuState) {
    core::arch::asm!("fxsave [{}]", in(reg) state);
}

pub unsafe fn restore_fpu(state: *const FpuState) {
    core::arch::asm!("fxrstor [{}]", in(reg) state);
}

pub fn interrupts_enabled() -> bool {
    x86_64::instructions::interrupts::are_enabled()
}

pub fn enable_interrupts() {
    x86_64::instructions::interrupts::enable();
}

pub fn disable_interrupts() {
    x86_64::instructions::interrupts::disable();
}

pub fn without_interrupts<F, R>(f: F) -> R
where
    F: FnOnce() -> R,
{
    x86_64::instructions::interrupts::without_interrupts(f)
}

pub fn yield_now() {
    unsafe { core::arch::asm!("int 0x20"); }
}

pub fn init_process_stack(proc: &mut crate::process::Process) {
    use crate::memory::VirtAddr;
    use x86_64::structures::paging::FrameAllocator;
    
    let stack_top = proc.stack_top.as_u64();
    if stack_top == 0 { return; }

    // 1. Allocate and map the top page of the stack if not already mapped.
    let page_addr = (stack_top - 4096) & !0xFFF;
    let page = x86_64::structures::paging::Page::containing_address(VirtAddr::new(page_addr));
    let mut fa_guard = crate::memory::FRAME_ALLOCATOR.lock();
    let mut stack_frame_phys = None;
    let fa = fa_guard.as_mut().unwrap();
    let flags = x86_64::structures::paging::PageTableFlags::PRESENT 
              | x86_64::structures::paging::PageTableFlags::WRITABLE 
              | x86_64::structures::paging::PageTableFlags::USER_ACCESSIBLE;

    if let Some(frame) = fa.allocate_frame() {
        unsafe {
            use x86_64::structures::paging::Mapper;
            if let Some(pt_frame) = proc.page_table_frame {
                let po = crate::memory::paging::phys_offset();
                let l4_virt = po + pt_frame.start_address().as_u64();
                let l4 = &mut *(l4_virt.as_mut_ptr());
                let mut mapper = x86_64::structures::paging::OffsetPageTable::new(l4, po);
                let _ = mapper.map_to(page, frame, flags, fa).map(|flusher| flusher.flush());
                stack_frame_phys = Some(frame.start_address().as_u64());
            } else {
                let mut mapper = crate::memory::paging::current_mapper();
                let _ = mapper.map_to(page, frame, flags, fa).map(|flusher| flusher.flush());
                stack_frame_phys = Some(frame.start_address().as_u64());
            }
        }
    }
    
    // 2. ABI-specific stack initialization (argc, argv, envp, auxv).
    // In standard Linux x86_64 ABI:
    // [RSP]      = argc
    // [RSP+8]    = argv[0]
    // ...
    // [RSP+N*8]  = NULL
    // [RSP+(N+1)*8] = envp[0]
    // ...
    // [RSP+M*8]  = NULL
    // [RSP+(M+1)*8] = auxv[0].type
    // [RSP+(M+2)*8] = auxv[0].value
    // ...
    // [RSP+K*8]  = 0 (AT_NULL)
    
    let mut user_rsp = stack_top - 40;

    if let Some(stack_frame_phys) = stack_frame_phys {
        unsafe {
            let po = crate::memory::paging::phys_offset();

            // 1. Write the 16 random bytes at the very top of the user stack
            let random_data_size = 16u64;
            let random_data_sp = stack_top - random_data_size;
            let random_dst = (po + stack_frame_phys + (random_data_sp - page_addr)).as_mut_ptr::<u8>();
            // Write dummy random bytes
            for idx in 0..16 {
                random_dst.add(idx).write((idx + 7) as u8);
            }

            // 2. Write argv and envp strings below the random bytes
            let mut sp = random_data_sp;
            let mut argv_ptrs = alloc::vec::Vec::with_capacity(proc.argv.len());
            let mut envp_ptrs = alloc::vec::Vec::with_capacity(proc.envp.len());

            for s in proc.argv.iter().chain(proc.envp.iter()) {
                sp = sp.saturating_sub((s.len() + 1) as u64);
                let dst = (po + stack_frame_phys + (sp - page_addr)).as_mut_ptr::<u8>();
                core::ptr::copy_nonoverlapping(s.as_ptr(), dst, s.len());
                dst.add(s.len()).write(0);
                if argv_ptrs.len() < proc.argv.len() {
                    argv_ptrs.push(sp);
                } else {
                    envp_ptrs.push(sp);
                }
            }

            // Align stack pointer to 16 bytes
            sp &= !0xFu64;

            // 3. Compute SP for the pointers
            // We will push:
            // - argc (1 u64)
            // - argv pointers (argv_ptrs.len() + 1 u64)
            // - envp pointers (envp_ptrs.len() + 1 u64)
            // - auxv entries:
            //   - AT_RANDOM (tag 25, value: random_data_sp)
            //   - AT_PAGESZ (tag 6, value: 4096)
            //   - Redox magic tag 42 (tag 42, value: 31337)
            //   - AT_NULL (tag 0, value: 0)
            // Each auxv entry is 2 u64s (tag, value).
            // Number of auxv entries = 4. So 4 * 2 = 8 u64s.
            let ptrs_count = 1 + argv_ptrs.len() + 1 + envp_ptrs.len() + 1 + (4 * 2);
            user_rsp = sp.saturating_sub((ptrs_count * 8) as u64) & !0xFu64;

            let mut ptr = (po + stack_frame_phys + (user_rsp - page_addr)).as_mut_ptr::<u64>();
            
            // argc
            ptr.write(argv_ptrs.len() as u64);
            ptr = ptr.add(1);

            // argv
            for p in argv_ptrs {
                ptr.write(p);
                ptr = ptr.add(1);
            }
            ptr.write(0);
            ptr = ptr.add(1);

            // envp
            for p in envp_ptrs {
                ptr.write(p);
                ptr = ptr.add(1);
            }
            ptr.write(0);
            ptr = ptr.add(1);

            // auxv
            // AT_RANDOM (25)
            ptr.write(25);
            ptr.add(1).write(random_data_sp);
            ptr = ptr.add(2);

            // AT_PAGESZ (6)
            ptr.write(6);
            ptr.add(1).write(4096);
            ptr = ptr.add(2);

            // Redox magic tag 42 (42)
            ptr.write(42);
            ptr.add(1).write(31337);
            ptr = ptr.add(2);

            // AT_NULL (0)
            ptr.write(0);
            ptr.add(1).write(0);
        }
    }

    // 3. Initialize kernel stack with CpuState for jump_to_user_stub
    if proc.kernel_stack_top != 0 {
        unsafe {
            let kstack_top = proc.kernel_stack_top;
            let cpu_state_size = core::mem::size_of::<crate::process::CpuState>() as u64;
            let cpu_state_ptr = (kstack_top - cpu_state_size) as *mut crate::process::CpuState;
            
            cpu_state_ptr.write(crate::process::CpuState { 
                rax: 0, rcx: 0, rdx: 0, rbx: 0, rbp: 0, rsi: 0, rdi: 0, r8: 0, 
                r9: 0, r10: 0, r11: 0, r12: 0, r13: 0, r14: 0, r15: 0, 
                rip: proc.entry_point.as_u64(), 
                cs: crate::arch::x86_64::gdt::user_code_selector().0 as u64, 
                rflags: 0x202, 
                rsp: user_rsp, // USE THE NEW USER RSP
                ss: crate::arch::x86_64::gdt::user_data_selector().0 as u64, 
            });
            
            let ret_addr_ptr = (kstack_top - cpu_state_size - 8) as *mut u64;
            ret_addr_ptr.write(jump_to_user_stub as *const () as u64);
            
            // Context for switch_context (pushes rbx, rbp, r12, r13, r14, r15 = 6 regs = 48 bytes)
            let context_ptr = (kstack_top - cpu_state_size - 8 - 48) as *mut u64;
            for i in 0..6 { context_ptr.add(i).write(0); }
            
            proc.kernel_stack_ptr = kstack_top - cpu_state_size - 8 - 48;
        }
    }
}

pub struct CpuState {
    // Pushed by assembly stub (pop order)
    pub rax: u64,
    pub rcx: u64,
    pub rdx: u64,
    pub rbx: u64,
    pub rbp: u64,
    pub rsi: u64,
    pub rdi: u64,
    pub r8: u64,
    pub r9: u64,
    pub r10: u64,
    pub r11: u64,
    pub r12: u64,
    pub r13: u64,
    pub r14: u64,
    pub r15: u64,

    // Pushed automatically by CPU on interrupt
    pub rip: u64,
    pub cs: u64,
    pub rflags: u64,
    pub rsp: u64,
    pub ss: u64,
}

impl CpuState {
    pub const fn zero() -> Self {
        Self {
            rax: 0,
            rcx: 0,
            rdx: 0,
            rbx: 0,
            rbp: 0,
            rsi: 0,
            rdi: 0,
            r8: 0,
            r9: 0,
            r10: 0,
            r11: 0,
            r12: 0,
            r13: 0,
            r14: 0,
            r15: 0,
            rip: 0,
            rflags: 0x202,
            cs: 8,
            ss: 0,
            rsp: 0,
        }
    }
}

pub fn new_user_state(entry: crate::memory::VirtAddr, stack: crate::memory::VirtAddr) -> CpuState {
    let mut cpu = CpuState::zero();
    cpu.rip = entry.as_u64();
    cpu.rsp = stack.as_u64();
    cpu.cs = gdt::user_code_selector().0 as u64;
    cpu.ss = gdt::user_data_selector().0 as u64;
    cpu
}

pub fn init_kthread_stack(proc: &mut crate::process::Process, entry: u64, arg: u64) {
    if let Some(ref mut _kstack) = proc.kernel_stack {
        let kstack_top = proc.kernel_stack_top;
        unsafe {
            // Layout for the first `switch_context` into this kthread.
            // `switch_context` saves 6 callee-saved regs in the order
            //   push rbp, rbx, r12, r13, r14, r15
            // and pops them in reverse: r15, r14, r13, r12, rbx, rbp.
            let slots = (kstack_top - 56) as *mut u64;
            slots.add(0).write(0);                  // rbp
            slots.add(1).write(0);                  // rbx
            slots.add(2).write(entry);              // r13 -> entry
            slots.add(3).write(arg);                // r12 -> arg
            slots.add(4).write(0);                  // r14
            slots.add(5).write(0);                  // r15
            // Return address for the kthread's first `switch_context` ret.
            (kstack_top as *mut u64).sub(1).write(proc.entry_point.as_u64());
            proc.kernel_stack_ptr = kstack_top - 56;
        }
    }
}

#[unsafe(naked)]
pub unsafe extern "C" fn kthread_trampoline() -> ! {
    core::arch::naked_asm!(
        "
        sti
        mov rdi, r12
        call r13
        xor edi, edi
        jmp kthread_exit_trampoline
        "
    );
}

/// Alias for kthread_trampoline — used by scheduler to get its address
pub unsafe extern "C" fn kthread_trampoline_wrapper() -> ! {
    unsafe { kthread_trampoline() }
}

/// Atomically update the stacks used for Ring 3 -> Ring 0 transitions.
/// This includes both the TSS.RSP0 (for interrupts) and the KERNEL_STACK 
/// global (for syscalls).
pub fn update_trap_stacks(stack_top: u64) {
    x86_64::instructions::interrupts::without_interrupts(|| {
        gdt::set_tss_stack(x86_64::VirtAddr::new(stack_top));
        switch::set_kernel_stack(stack_top);
    });
}
