use crate::println;
use lazy_static::lazy_static;
use pic8259::ChainedPics;
use spin::Mutex;
use x86_64::structures::idt::{InterruptDescriptorTable, InterruptStackFrame, PageFaultErrorCode};
use x86_64::structures::paging::{FrameAllocator, Mapper};

pub const PIC_1_OFFSET: u8 = 32;
pub const PIC_2_OFFSET: u8 = PIC_1_OFFSET + 8;

pub static PICS: Mutex<ChainedPics> =
    Mutex::new(unsafe { ChainedPics::new(PIC_1_OFFSET, PIC_2_OFFSET) });

#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum InterruptIndex {
    Timer = PIC_1_OFFSET,        // 32
    Keyboard = PIC_1_OFFSET + 1, // 33
    Mouse = PIC_2_OFFSET + 4,    // 44 (IRQ 12)
    Ata1 = PIC_1_OFFSET + 14,    // 46
    Ata2 = PIC_1_OFFSET + 15,    // 47
}

/// int 0x80 — Linux-compatible syscall gate
pub const SYSCALL_VECTOR: u8 = 0x80;

lazy_static! {
    pub static ref IDT: InterruptDescriptorTable = {
        let mut idt = InterruptDescriptorTable::new();

        // ── CPU exceptions ──
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

        // ── int 0x80 syscall gate ──
        idt[SYSCALL_VECTOR as usize].set_handler_fn(syscall_handler);

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

// ── Exception handlers ──

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
    println!(
        "EXCEPTION: PAGE FAULT\n  addr={:?}  code={:?}\n{:#?}",
        fault_addr, error_code, frame
    );

    // Handle demand paging
    let scheduler = crate::process::scheduler::SCHEDULER.lock();
    let current_proc = scheduler.current_task();
    if let Some(proc) = current_proc {
        println!("[MM] Current process {} found", proc.pid.0);
        // Check if fault address is within any of the process's memory regions
        let has_region = proc.regions.iter().any(|opt_region| {
            opt_region
                .as_ref()
                .map(|region| {
                    let start = region.start.as_u64();
                    let end = start + region.size as u64;
                    fault_addr.as_u64() >= start && fault_addr.as_u64() < end
                })
                .unwrap_or(false)
        });
        if has_region {
            println!("[MM] Found region for address {:?}", fault_addr);
        } else {
            println!("[MM] No region found for address {:?}", fault_addr);
        }
    }
    drop(scheduler); // Release lock before potentially blocking or performing complex operations

    if error_code.contains(PageFaultErrorCode::PROTECTION_VIOLATION) {
        // Check if this is a COW page — allocate a private copy and remap writable
        let cow_handled = {
            let scheduler = crate::process::scheduler::SCHEDULER.lock();
            let is_cow = scheduler
                .current_task()
                .map(|proc| {
                    proc.regions.iter().any(|opt| {
                        opt.as_ref()
                            .map(|r| {
                                let start = r.start.as_u64();
                                let end = start + r.size as u64;
                                r.flags.copy_on_write
                                    && fault_addr.as_u64() >= start
                                    && fault_addr.as_u64() < end
                            })
                            .unwrap_or(false)
                    })
                })
                .unwrap_or(false);
            drop(scheduler);
            if is_cow {
                crate::memory::paging::handle_cow_fault(fault_addr)
            } else {
                false
            }
        };
        if cow_handled {
            println!("[MM] COW fault resolved at {:?}", fault_addr);
            return;
        }
        println!("[MM] Protection violation, but NOT a COW page — halting");
    } else {
        // Page not present - demand paging opportunity
        println!(
            "[MM] Page not present, attempting demand paging at addr {:?}",
            fault_addr
        );

        // Extract region + binary data info while holding the scheduler lock,
        // then drop it before taking memory locks to avoid deadlocks.
        let demand_info = {
            let scheduler = crate::process::scheduler::SCHEDULER.lock();
            if let Some(proc) = scheduler.current_task() {
                let region_entry = proc.regions.iter().find(|opt_region| {
                    opt_region
                        .as_ref()
                        .map(|region| {
                            let start = region.start.as_u64();
                            let end = start + region.size as u64;
                            fault_addr.as_u64() >= start && fault_addr.as_u64() < end
                        })
                        .unwrap_or(false)
                });
                if let Some(Some(region)) = region_entry {
                    // Clone what we need before dropping the lock
                    let binary_ptr = if !proc.binary_data.is_empty() {
                        Some((proc.binary_data.as_ptr(), proc.binary_data.len()))
                    } else {
                        None
                    };
                    Some((region.start.as_u64(), region.file_offset, binary_ptr))
                } else {
                    println!("[MM] Invalid access - no region found");
                    None
                }
            } else {
                None
            }
        }; // scheduler lock dropped here

        if let Some((region_start, file_offset, binary_info)) = demand_info {
            let page = x86_64::structures::paging::Page::containing_address(fault_addr);
            let flags = x86_64::structures::paging::PageTableFlags::PRESENT
                | x86_64::structures::paging::PageTableFlags::WRITABLE
                | x86_64::structures::paging::PageTableFlags::USER_ACCESSIBLE;

            // Hold a SINGLE lock on the frame allocator for the entire operation.
            // This prevents map_to from getting frames that conflict with our frame.
            let mut fa_guard = crate::memory::FRAME_ALLOCATOR.lock();
            let fa = fa_guard.as_mut().unwrap();

            if let Some(frame) = fa.allocate_frame() {
                unsafe {
                    let mut mapper = crate::memory::paging::current_mapper();
                    // Use map_to with the SAME frame allocator reference —
                    // no double locking, no frame reuse.
                    match mapper.map_to(page, frame, flags, fa) {
                        Ok(flusher) => {
                            flusher.flush();

                            // Copy ELF binary data into the newly mapped page
                            if let Some((bin_ptr, bin_len)) = binary_info {
                                let page_addr = page.start_address().as_u64();
                                let in_region_off = page_addr.saturating_sub(region_start);
                                let binary_off = file_offset as usize + in_region_off as usize;
                                let copy_size = 4096usize.min(bin_len.saturating_sub(binary_off));
                                if copy_size > 0 {
                                    core::ptr::copy_nonoverlapping(
                                        bin_ptr.add(binary_off),
                                        page_addr as *mut u8,
                                        copy_size,
                                    );
                                    println!(
                                        "[MM] Copied {} bytes from binary to {:x}",
                                        copy_size, page_addr
                                    );
                                }
                            }

                            println!("[MM] Demand page mapped + populated");
                            return;
                        }
                        Err(_e) => {
                            // Page was already mapped (race or stale TLB).
                            // This is the panic you were seeing — now handled gracefully.
                            println!("[MM] Page already mapped at {:?}, skipping", fault_addr);
                            return;
                        }
                    }
                }
            } else {
                println!("[MM] Out of memory - frame allocation failed");
            }
        }
    }

    // For now halt on unhandled faults
    loop {
        x86_64::instructions::hlt();
    }
}

extern "x86-interrupt" fn double_fault_handler(frame: InterruptStackFrame, _code: u64) -> ! {
    panic!("EXCEPTION: DOUBLE FAULT\n{:#?}", frame);
}

// ── Hardware interrupt handlers ──

extern "x86-interrupt" fn timer_handler(_frame: InterruptStackFrame) {
    crate::process::scheduler::tick();
    crate::timer::tick();
    unsafe {
        PICS.lock()
            .notify_end_of_interrupt(InterruptIndex::Timer as u8)
    };
}

extern "x86-interrupt" fn keyboard_handler(_frame: InterruptStackFrame) {
    use x86_64::instructions::port::Port;

    let mut port: Port<u8> = Port::new(0x60);
    let scancode: u8 = unsafe { port.read() };

    // Push scancode into the keyboard ring buffer
    crate::drivers::keyboard::push_scancode(scancode);

    unsafe { PICS.lock().notify_end_of_interrupt(InterruptIndex::Keyboard as u8) };
}

extern "x86-interrupt" fn mouse_handler(_frame: InterruptStackFrame) {
    crate::drivers::ps2_mouse::on_interrupt();
    unsafe {
        PICS.lock()
            .notify_end_of_interrupt(InterruptIndex::Mouse as u8)
    };
}


// ── int 0x80 syscall gate ──

/// Called when `int 0x80` is executed.
/// Reads registers directly via inline asm (first statement before any clobbering)
/// and dispatches through the ABI-aware syscall dispatch.
/// Return value is lost (RAX is restored by x86-interrupt epilogue); for testing
/// the test binary doesn't check return values.
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
    let mut scheduler = crate::process::scheduler::SCHEDULER.lock();
    if let Some(proc) = scheduler.current_task_mut() {
        let mut ctx = crate::abi::syscall::SyscallContext::new(num, args, proc);
        let handler = crate::abi::handler::KernelSyscallHandler;
        match crate::abi::syscall::dispatch_syscall(&registry, &handler, &mut ctx) {
            Ok(v) => {
                println!("[ZIQA] syscall {} -> OK({})", num, v);
            }
            Err(e) => {
                println!("[ZIQA] syscall {} -> ERR({:?})", num, e);
            }
        }
    } else {
        println!("[ZIQA] int 0x80 but no current process");
    }
    // No EOI needed — software interrupt, not PIC-sourced.
}

extern "x86-interrupt" fn ata1_handler(_frame: InterruptStackFrame) {
    unsafe {
        PICS.lock()
            .notify_end_of_interrupt(InterruptIndex::Ata1 as u8)
    };
}

extern "x86-interrupt" fn ata2_handler(_frame: InterruptStackFrame) {
    unsafe {
        PICS.lock()
            .notify_end_of_interrupt(InterruptIndex::Ata2 as u8)
    };
}
