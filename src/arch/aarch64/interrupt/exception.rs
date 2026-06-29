/// ARM64 exception and interrupt handlers
/// Ported from Redox OS and adapted for ZiqaKernel.

use crate::println;
use crate::arch::aarch64::interrupt::InterruptStack;
use crate::memory::paging::MemoryRegionFlags;

fn exception_code(esr: u64) -> u8 {
    ((esr >> 26) & 0x3f) as u8
}

fn iss(esr: u64) -> u32 {
    (esr & 0x01ff_ffff) as u32
}

unsafe fn far_el1() -> u64 {
    let ret: u64;
    core::arch::asm!("mrs {}, far_el1", out(reg) ret);
    ret
}

pub fn synchronous_exception_at_el1_with_sp0(stack: &mut InterruptStack) {
    println!("Synchronous exception at EL1 with SP0");
    stack.dump();
    loop {}
}

pub fn synchronous_exception_at_el1_with_spx(stack: &mut InterruptStack) {
    let esr = stack.iret.esr_el1 as u64;
    let code = exception_code(esr);
    
    // Handle Page Faults (Data/Instruction Aborts)
    if code == 0b100101 || code == 0b100100 || code == 0b100001 || code == 0b100000 {
        let far = unsafe { far_el1() };
        println!("Page Fault at EL1: FAR=0x{:x}, ESR=0x{:x}", far, esr);
        // TODO: Call ZiqaKernel page fault handler
    } else {
        println!("Synchronous exception at EL1 with SPx: Code=0x{:x}", code);
        stack.dump();
    }
    loop {}
}

pub fn synchronous_exception_at_el0(stack: &mut InterruptStack) {
    let esr = stack.iret.esr_el1 as u64;
    let code = exception_code(esr);

    match code {
        0b010101 => { // SVC (System Call)
            let num = stack.scratch.x8 as u64;
            let args = [
                stack.scratch.x0 as u64,
                stack.scratch.x1 as u64,
                stack.scratch.x2 as u64,
                stack.scratch.x3 as u64,
                stack.scratch.x4 as u64,
                stack.scratch.x5 as u64,
            ];
            
            // Forward to ZiqaKernel syscall dispatcher
            // This will be connected once the syscall module is ready for ARM64
            println!("ARM64 Syscall: {}", num);
            stack.scratch.x0 = 0; // Success for now
        }
        _ => {
            println!("Unhandled exception at EL0: Code=0x{:x}", code);
            stack.dump();
            loop {}
        }
    }
}

pub fn unhandled_exception(stack: &mut InterruptStack) {
    println!("Unhandled exception");
    stack.dump();
    loop {}
}
