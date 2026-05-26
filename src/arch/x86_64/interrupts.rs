use pic8259::ChainedPics;
use spin::Mutex;
use x86_64::structures::idt::{InterruptDescriptorTable, InterruptStackFrame, PageFaultErrorCode};
use lazy_static::lazy_static;
use crate::println;

pub const PIC_1_OFFSET: u8 = 32;
pub const PIC_2_OFFSET: u8 = PIC_1_OFFSET + 8;

pub static PICS: Mutex<ChainedPics> =
    Mutex::new(unsafe { ChainedPics::new(PIC_1_OFFSET, PIC_2_OFFSET) });

#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum InterruptIndex {
    Timer    = PIC_1_OFFSET,      // 32
    Keyboard = PIC_1_OFFSET + 1,  // 33
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
    println!("EXCEPTION: GENERAL PROTECTION FAULT (code={:#x})\n{:#?}", error_code, frame);
    // In a real kernel we'd kill the offending process; for now halt.
    loop { x86_64::instructions::hlt(); }
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
    if error_code.contains(PageFaultErrorCode::PROTECTION_VIOLATION) {
        // Protection violation - likely a write to a read-only page (copy-on-write)
        println!("[MM] Protection violation, handling copy-on-write");
        // TODO: Implement copy-on-write
    } else {
        // Page not present - demand paging opportunity
        println!("[MM] Page not present, attempting demand paging");
        // TODO: Implement demand paging
    }
    
    // For now halt on unhandled faults
    loop { x86_64::instructions::hlt(); }
}

extern "x86-interrupt" fn double_fault_handler(frame: InterruptStackFrame, _code: u64) -> ! {
    panic!("EXCEPTION: DOUBLE FAULT\n{:#?}", frame);
}

// ── Hardware interrupt handlers ──

extern "x86-interrupt" fn timer_handler(_frame: InterruptStackFrame) {
    crate::process::scheduler::tick();
    crate::timer::tick();
    unsafe { PICS.lock().notify_end_of_interrupt(InterruptIndex::Timer as u8) };
}

extern "x86-interrupt" fn keyboard_handler(_frame: InterruptStackFrame) {
    use x86_64::instructions::port::Port;

    let mut port: Port<u8> = Port::new(0x60);
    let scancode: u8 = unsafe { port.read() };

    // Push scancode into the keyboard ring buffer
    crate::drivers::keyboard::push_scancode(scancode);

    unsafe { PICS.lock().notify_end_of_interrupt(InterruptIndex::Keyboard as u8) };

}

// ── int 0x80 syscall gate ──

/// Called when user-mode code executes `int 0x80`.
/// Syscall number in RAX; args in RBX, RCX, RDX, RSI, RDI, RBP (Linux i386 convention).
/// For x86_64 Linux ABI (via `syscall` instruction) we handle it the same way here.
extern "x86-interrupt" fn syscall_handler(_frame: InterruptStackFrame) {
    // In a full implementation we'd read RAX from the saved frame and dispatch.
    // The ABI registry dispatch happens at a higher level (abi::syscall::dispatch_syscall).
    // This stub acknowledges the interrupt and returns.
    println!("[ZIQA] int 0x80 syscall gate hit");
    // No EOI needed — software interrupt, not PIC-sourced.
}
