/// RISC-V exception and interrupt handlers
/// Ported from Redox OS and adapted for ZiqaKernel.

use crate::println;
use crate::arch::riscv64::interrupt::InterruptStack;

pub unsafe fn exception_handler_inner(stack: &mut InterruptStack) {
    let scause: usize;
    let sstatus: usize;
    let stval: usize;
    core::arch::asm!(
        "csrr {}, scause",
        "csrr {}, sstatus",
        "csrr {}, stval",
        out(reg) scause,
        out(reg) sstatus,
        out(reg) stval,
    );

    let is_interrupt = (scause >> (usize::BITS - 1)) != 0;
    let code = scause & !(1 << (usize::BITS - 1));

    if is_interrupt {
        // Handle external/timer/software interrupts
        println!("RISC-V Interrupt: {}", code);
    } else {
        match code {
            8 => { // Environment call from U-mode (System Call)
                let num = stack.registers.x17 as u64;
                let args = [
                    stack.registers.x10 as u64,
                    stack.registers.x11 as u64,
                    stack.registers.x12 as u64,
                    stack.registers.x13 as u64,
                    stack.registers.x14 as u64,
                    stack.registers.x15 as u64,
                ];
                println!("RISC-V Syscall: {}", num);
                stack.iret.sepc += 4; // Skip ecall instruction
                stack.registers.x10 = 0; // Success for now
            }
            12 | 13 | 15 => { // Page Faults
                println!("RISC-V Page Fault: code={}, addr=0x{:x}, epc=0x{:x}", 
                    code, stval, stack.iret.sepc);
                // TODO: Call ZiqaKernel page fault handler
                loop {}
            }
            _ => {
                println!("Unhandled RISC-V Exception: code={}, stval=0x{:x}", code, stval);
                stack.dump();
                loop {}
            }
        }
    }
}

#[unsafe(naked)]
#[no_mangle]
pub unsafe extern "C" fn exception_handler() {
    core::arch::naked_asm!(
        // Save registers
        push_registers!(),
        
        // Call Rust handler
        "mv a0, sp",
        "jal exception_handler_inner",
        
        // Restore registers and return
        pop_registers!(),
        "sret"
    );
}
