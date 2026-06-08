/// RISC-V 64 reset and shutdown
/// Ported from Redox OS.

pub fn kreset() -> ! {
    println!("kreset (riscv64)");
    loop {
        unsafe { core::arch::asm!("wfi") };
    }
}

pub fn kstop() -> ! {
    println!("kstop (riscv64)");
    loop {
        unsafe { core::arch::asm!("wfi") };
    }
}
