pub mod handler;
pub mod exception;

use self::exception::*;

pub fn init() {
    unsafe {
        let base = exception_handler as usize;
        core::arch::asm!("csrw stvec, {}", in(reg) base);
    }
}
