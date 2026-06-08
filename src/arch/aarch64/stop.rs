/// AArch64 reset and shutdown
/// Ported from Redox OS.

pub fn kreset() -> ! {
    println!("kreset (aarch64)");
    loop {
        unsafe { core::arch::asm!("wfi") };
    }
}

pub fn kstop() -> ! {
    println!("kstop (aarch64)");
    loop {
        unsafe { core::arch::asm!("wfi") };
    }
}
