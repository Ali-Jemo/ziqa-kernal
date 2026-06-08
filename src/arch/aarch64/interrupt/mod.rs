pub mod handler;
pub mod exception;

use self::exception::*;

exception_stack!(sync_exc_el1_sp0, synchronous_exception_at_el1_with_sp0);
exception_stack!(sync_exc_el1_spx, synchronous_exception_at_el1_with_spx);
exception_stack!(sync_exc_el0, synchronous_exception_at_el0);
exception_stack!(irq_exc_el1_sp0, unhandled_exception);
exception_stack!(irq_exc_el1_spx, unhandled_exception);
exception_stack!(irq_exc_el0, unhandled_exception);
exception_stack!(fiq_exc_el1_sp0, unhandled_exception);
exception_stack!(fiq_exc_el1_spx, unhandled_exception);
exception_stack!(fiq_exc_el0, unhandled_exception);
exception_stack!(serror_exc_el1_sp0, unhandled_exception);
exception_stack!(serror_exc_el1_spx, unhandled_exception);
exception_stack!(serror_exc_el0, unhandled_exception);

#[unsafe(naked)]
#[no_mangle]
pub unsafe extern "C" fn exception_vector_base() {
    core::arch::naked_asm!(
        ".align 11",
        // Current EL with SP0
        "b sync_exc_el1_sp0",   ".align 7",
        "b irq_exc_el1_sp0",    ".align 7",
        "b fiq_exc_el1_sp0",    ".align 7",
        "b serror_exc_el1_sp0", ".align 7",

        // Current EL with SPx
        "b sync_exc_el1_spx",   ".align 7",
        "b irq_exc_el1_spx",    ".align 7",
        "b fiq_exc_el1_spx",    ".align 7",
        "b serror_exc_el1_spx", ".align 7",

        // Lower EL using AArch64
        "b sync_exc_el0",       ".align 7",
        "b irq_exc_el0",        ".align 7",
        "b fiq_exc_el0",        ".align 7",
        "b serror_exc_el0",     ".align 7",

        // Lower EL using AArch32
        "b unhandled_exception", ".align 7",
        "b unhandled_exception", ".align 7",
        "b unhandled_exception", ".align 7",
        "b unhandled_exception", ".align 7",
    );
}

pub fn init() {
    unsafe {
        let base = exception_vector_base as usize;
        core::arch::asm!("msr vbar_el1, {}", in(reg) base);
    }
}
