use crate::println;
use lazy_static::lazy_static;
use pic8259::ChainedPics;
use spin::Mutex;
use x86_64::structures::idt::{InterruptDescriptorTable, InterruptStackFrame, PageFaultErrorCode};
use x86_64::structures::paging::{FrameAllocator, Mapper, PageTableFlags};
#[cfg(feature = "ebpf")]
use crate::ebpf::attach::{EBPF_ATTACHMENTS, TracepointType};
use crate::abi::syscall::abi_error_to_errno;

pub const PIC_1_OFFSET: u8 = 32;
pub const PIC_2_OFFSET: u8 = PIC_1_OFFSET + 8;

pub static PICS: Mutex<ChainedPics> =
    Mutex::new(unsafe { ChainedPics::new(PIC_1_OFFSET, PIC_2_OFFSET) });

lazy_static! {
    pub static ref IRQ_WAITERS: Mutex<BTreeMap<u8, crate::process::Pid>> = Mutex::new(BTreeMap::new());
}

use alloc::collections::BTreeMap;

fn notify_irq(vector: u8) {
    crate::scheme::irq::irq_trigger(vector);
    let mut waiters = IRQ_WAITERS.lock();
    if let Some(pid) = waiters.remove(&vector) {
        crate::process::scheduler::wake_sleeping(1 << pid.0);
    }
}

#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum InterruptIndex {
    Timer = 0x20,                      // 32
    Keyboard = 0x21,                   // 33
    IpIReschedule = 0x34,              // 52
    IpITlbShootdown = 0x35,            // 53
    Mouse = 0x2C,                      // 44 (IRQ 12)
    Ata1 = 0x2E,                       // 46 (IRQ 14)
    Ata2 = 0x2F,                       // 47 (IRQ 15)
    Spurious = 0x30,                   // 48
    ApicError = 0x31,                  // 49
}

pub const SYSCALL_VECTOR: u8 = 0x80;

lazy_static! {
    pub static ref IDT: InterruptDescriptorTable = {
        let mut idt = InterruptDescriptorTable::new();

        // ── CPU exceptions ──
        idt.divide_error.set_handler_fn(divide_by_zero_handler);
        idt.invalid_opcode.set_handler_fn(invalid_opcode_handler);
        idt.stack_segment_fault.set_handler_fn(stack_segment_fault_handler);
        idt.invalid_tss.set_handler_fn(invalid_tss_handler);
        idt.segment_not_present.set_handler_fn(segment_not_present_handler);
        idt.breakpoint.set_handler_fn(breakpoint_handler);
        idt.general_protection_fault.set_handler_fn(gpf_handler);
        idt.page_fault.set_handler_fn(page_fault_handler);
        unsafe {
            idt.double_fault
                .set_handler_fn(double_fault_handler)
                .set_stack_index(crate::arch::x86_64::gdt::DOUBLE_FAULT_IST_INDEX);
        }

        // ── Hardware interrupts ──
        idt[InterruptIndex::Timer as usize].set_handler_fn(timer_handler);
        idt[InterruptIndex::Keyboard as usize].set_handler_fn(keyboard_handler);
        idt[InterruptIndex::Mouse as usize].set_handler_fn(mouse_handler);
        idt[InterruptIndex::Ata1 as usize].set_handler_fn(ata1_handler);
        idt[InterruptIndex::Ata2 as usize].set_handler_fn(ata2_handler);

        // ── IPI handlers ──
        idt[InterruptIndex::IpIReschedule as usize].set_handler_fn(ipi_reschedule_handler);
        idt[InterruptIndex::IpITlbShootdown as usize].set_handler_fn(ipi_tlb_shootdown_handler);

        // ── int 0x80 syscall gate ──
        idt[SYSCALL_VECTOR as usize].set_handler_fn(syscall_handler);

        // ── Generic/Unhandled interrupt handlers ──
        for vector in 32..255 {
            if vector == InterruptIndex::Timer as usize
                || vector == InterruptIndex::Keyboard as usize
                || vector == InterruptIndex::Mouse as usize
                || vector == InterruptIndex::Ata1 as usize
                || vector == InterruptIndex::Ata2 as usize
                || vector == InterruptIndex::IpIReschedule as usize
                || vector == InterruptIndex::IpITlbShootdown as usize
                || vector == SYSCALL_VECTOR as usize
            {
                continue;
            }
            idt[vector].set_handler_fn(generic_interrupt_handler);
        }

        idt
    };
}

pub fn init_idt() {
    IDT.load();
}

pub fn init_pics() {
    unsafe { PICS.lock().initialize() };
    x86_64::instructions::interrupts::enable();
}

fn send_eoi(vector: u8) {
    unsafe {
        if crate::arch::x86_64::apic::LAPIC_VADDR != 0 {
            crate::arch::x86_64::apic::eoi();
        } else {
            PICS.lock().notify_end_of_interrupt(vector);
        }
    }
}

// ── Exception handlers ──

extern "x86-interrupt" fn divide_by_zero_handler(frame: InterruptStackFrame) {
    println!("EXCEPTION: DIVIDE BY ZERO\n{:#?}", frame);
    loop { x86_64::instructions::hlt(); }
}

extern "x86-interrupt" fn invalid_opcode_handler(frame: InterruptStackFrame) {
    println!("EXCEPTION: INVALID OPCODE\n{:#?}", frame);
    loop { x86_64::instructions::hlt(); }
}

extern "x86-interrupt" fn stack_segment_fault_handler(frame: InterruptStackFrame, error_code: u64) {
    println!("EXCEPTION: STACK SEGMENT FAULT (code={:#x})\n{:#?}", error_code, frame);
    loop { x86_64::instructions::hlt(); }
}

extern "x86-interrupt" fn invalid_tss_handler(frame: InterruptStackFrame, error_code: u64) {
    println!("EXCEPTION: INVALID TSS (code={:#x})\n{:#?}", error_code, frame);
    loop { x86_64::instructions::hlt(); }
}

extern "x86-interrupt" fn segment_not_present_handler(frame: InterruptStackFrame, error_code: u64) {
    println!("EXCEPTION: SEGMENT NOT PRESENT (code={:#x})\n{:#?}", error_code, frame);
    loop { x86_64::instructions::hlt(); }
}

extern "x86-interrupt" fn breakpoint_handler(frame: InterruptStackFrame) {
    println!("EXCEPTION: BREAKPOINT\n{:#?}", frame);
}

extern "x86-interrupt" fn gpf_handler(frame: InterruptStackFrame, error_code: u64) {
    println!(
        "EXCEPTION: GENERAL PROTECTION FAULT (code={:#x})\n{:#?}",
        error_code, frame
    );
    // In a real kernel we'd kill the offending process; for now halt.
    loop {
        x86_64::instructions::hlt();
    }
}

extern "x86-interrupt" fn page_fault_handler(
    frame: InterruptStackFrame,
    error_code: PageFaultErrorCode,
) {
    use x86_64::registers::control::Cr2;
    let fault_addr = Cr2::read();
    
    // Fast path: Check if this is a compressed page fault
    if crate::memory::compression::fault::handle_compressed_fault(fault_addr, error_code) {
        return;
    }
    
    let handled = crate::process::scheduler::with_current_task(|proc| {
        let access_flags = if error_code.contains(PageFaultErrorCode::PROTECTION_VIOLATION) {
             PageTableFlags::WRITABLE 
        } else {
            PageTableFlags::empty()
        };

        // Find matching VMA
        let vma = match proc.vmas.iter().find(|v| v.contains(fault_addr)) {
            Some(v) => v,
            None => return false,
        };

        // Verify permissions
        if access_flags.contains(PageTableFlags::WRITABLE) && !vma.flags.writable {
            return false;
        }

        let pt_frame = match proc.page_table_frame {
            Some(f) => f,
            None => return false,
        };

        // Handle COW fault
        if vma.flags.copy_on_write && access_flags.contains(PageTableFlags::WRITABLE) {
            return crate::memory::paging::handle_cow_fault(pt_frame, fault_addr);
        }

        // Demand page
        let page_flags = crate::memory::paging::region_flags_to_page_flags(&vma.flags);
        crate::memory::paging::demand_page_for_frame(pt_frame, fault_addr, page_flags)
    }).unwrap_or(false);

    if handled {
        return;
    }

    println!(
        "EXCEPTION: PAGE FAULT (Unresolved)\n  addr={:?}  code={:?}\n{:#?}",
        fault_addr, error_code, frame
    );

    loop {
        x86_64::instructions::hlt();
    }
}

extern "x86-interrupt" fn double_fault_handler(frame: InterruptStackFrame, _code: u64) -> ! {
    panic!("EXCEPTION: DOUBLE FAULT\n{:#?}", frame);
}

extern crate alloc;

// ── Hardware interrupt handlers ──

extern "x86-interrupt" fn timer_handler(_frame: InterruptStackFrame) {
    notify_irq(InterruptIndex::Timer as u8);
    crate::arch::x86_64::per_cpu::current_cpu().tick();
    if crate::arch::x86_64::per_cpu::current_cpu().cpu_id == 0 {
        crate::process::scheduler::tick();
        crate::timer::tick();
    }
    send_eoi(InterruptIndex::Timer as u8);
}

extern "x86-interrupt" fn keyboard_handler(_frame: InterruptStackFrame) {
    notify_irq(InterruptIndex::Keyboard as u8);
    send_eoi(InterruptIndex::Keyboard as u8);
}

extern "x86-interrupt" fn mouse_handler(_frame: InterruptStackFrame) {
    notify_irq(InterruptIndex::Mouse as u8);
    crate::drivers::ps2_mouse::on_interrupt();
    send_eoi(InterruptIndex::Mouse as u8);
}

// ── IPI Handlers ──

extern "x86-interrupt" fn ipi_reschedule_handler(_frame: InterruptStackFrame) {
    send_eoi(InterruptIndex::IpIReschedule as u8);
    crate::arch::x86_64::smp::handle_reschedule_ipi();
}

extern "x86-interrupt" fn ipi_tlb_shootdown_handler(_frame: InterruptStackFrame) {
    send_eoi(InterruptIndex::IpITlbShootdown as u8);
    crate::arch::x86_64::smp::handle_tlb_shootdown();
}

// ── int 0x80 syscall gate ──

/// Called when `int 0x80` is executed.
/// Reads registers directly via inline asm (first statement before any clobbering)
/// and dispatches through the ABI-aware syscall handler.
/// Return value is set in RAX.
extern "x86-interrupt" fn syscall_handler(_frame: InterruptStackFrame) {
    let num: u64;
    let mut args: [u64; 6] = [0; 6];
    unsafe {
        core::arch::asm!(
            "mov {num}, rax",
            "mov {a0}, rdi",
            "mov {a1}, rsi",
            "mov {a2}, rdx",
            "mov {a3}, r10",
            "mov {a4}, r8",
            "mov {a5}, r9",
            num = out(reg) num,
            a0 = out(reg) args[0],
            a1 = out(reg) args[1],
            a2 = out(reg) args[2],
            a3 = out(reg) args[3],
            a4 = out(reg) args[4],
            a5 = out(reg) args[5],
            options(preserves_flags)
        );
    }

    let registry = crate::init_abi_registry();
    // Use the safe closure-based helper so the process lock is bracketed by
    // `without_interrupts`. The lock is only held while we're inside the
    // closure; once it returns, the timer ISR can safely re-lock the process
    // (or skip it via try_lock).
    let dispatch_result = crate::process::scheduler::with_current_task_mut(|proc| {
        let mut ctx = crate::abi::syscall::SyscallContext::new(num, args, proc);

        // Run entry tracepoints
        #[cfg(feature = "ebpf")]
        {
            use crate::ebpf::attach::{TracepointType, TracepointCtx};
            crate::ebpf::attach::EBPF_ATTACHMENTS.run(TracepointType::SyscallEntry, TracepointCtx::Syscall(ctx.info()));
        }

        let handler = crate::abi::handler::KernelSyscallHandler;
        let res = crate::abi::syscall::dispatch_syscall(&registry, &handler, &mut ctx);

        // Convert the result to the value that should be in RAX
        let retval = match &res {
            Ok(v) => *v,
            Err(e) => {
                // Convert AbiError to errno and then to the returned value (negative errno)
                let errno = abi_error_to_errno(e);
                (errno as i64).wrapping_neg() as u64
            }
        };

        // Update the context with the return value for exit tracepoints
        ctx.retval = retval;

        // Run exit tracepoints
        #[cfg(feature = "ebpf")]
        {
            use crate::ebpf::attach::{TracepointType, TracepointCtx};
            crate::ebpf::attach::EBPF_ATTACHMENTS.run(TracepointType::SyscallExit, TracepointCtx::Syscall(ctx.info()));
        }

        (retval, res)
    });

    match dispatch_result {
        Some((retval, res)) => {
            // Set RAX to the return value
            unsafe {
                core::arch::asm!("mov {}, rax", in(reg) retval, options(preserves_flags));
            }
            // Log the result (optional)
            match &res {
                Ok(v) => {
                    println!("[ZIQA] syscall {} -> OK({})", num, v);
                }
                Err(e) => {
                    println!("[ZIQA] syscall {} -> ERR({:?})", num, e);
                }
            }
        }
        None => {
            println!("[ZIQA] int 0x80 but no current process");
            // No EOI needed — software interrupt, not PIC-sourced.
        }
    }
}

extern "x86-interrupt" fn ata1_handler(_frame: InterruptStackFrame) {
    notify_irq(InterruptIndex::Ata1 as u8);
    send_eoi(InterruptIndex::Ata1 as u8);
}

extern "x86-interrupt" fn ata2_handler(_frame: InterruptStackFrame) {
    notify_irq(InterruptIndex::Ata2 as u8);
    send_eoi(InterruptIndex::Ata2 as u8);
}

extern "x86-interrupt" fn generic_interrupt_handler(frame: InterruptStackFrame) {
    let vector = unsafe {
        if crate::arch::x86_64::apic::LAPIC_VADDR != 0 {
            let mut active = None;
            for i in (0..8).rev() {
                let val = crate::arch::x86_64::apic::read_lapic(crate::arch::x86_64::apic::LAPIC_ISR_BASE + i * 0x10);
                if val != 0 {
                    let bit = 31 - val.leading_zeros();
                    active = Some((i * 32) as u8 + bit as u8);
                    break;
                }
            }
            active
        } else {
            None
        }
    };

    if let Some(v) = vector {
        crate::println!("UNHANDLED INTERRUPT: vector {} at rip={:?}", v, frame.instruction_pointer);
        send_eoi(v);
    } else {
        unsafe {
            if crate::arch::x86_64::apic::LAPIC_VADDR != 0 {
                crate::arch::x86_64::apic::eoi();
            }
        }
    }
}
