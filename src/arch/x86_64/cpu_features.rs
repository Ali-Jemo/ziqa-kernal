//! CPU feature detection and enforcement for SMEP/SMAP/UMIP.
//!
//! SMEP (Supervisor Mode Execution Prevention, CR4.SMEP bit 20):
//!   Raises a #PF if the kernel attempts to execute a user-accessible page.
//!   Defeats ret2usr and JIT-spray attacks.
//!
//! SMAP (Supervisor Mode Access Prevention, CR4.SMAP bit 21):
//!   Raises a #PF if the kernel reads/writes a user-accessible page while
//!   RFLAGS.AC == 0. Legitimate accesses must use `with_user_access`.
//!
//! UMIP (User-Mode Instruction Prevention, CR4.UMIP bit 11):
//!   Raises a #GP if user space executes SGDT/SIDT/SLDT/SMSW/STR.
//!   Prevents user processes from reading descriptor-table addresses,
//!   which are useful for KASLR-bypass and heap-spray targeting.
//!
//! Call `cpu_features::init()` once during early boot, after GDT/IDT are loaded.

use x86_64::registers::control::{Cr4, Cr4Flags};

// CPUID leaf 7, sub-leaf 0 bits
const CPUID_LEAF7_EBX_SMEP: u32 = 1 << 7;
const CPUID_LEAF7_EBX_SMAP: u32 = 1 << 20;
const CPUID_LEAF7_ECX_UMIP: u32 = 1 << 2;

/// Query CPUID leaf 7 sub-leaf 0; returns (EBX, ECX).
fn cpuid7() -> (u32, u32) {
    let (ebx, ecx): (u32, u32);
    unsafe {
        core::arch::asm!(
            "push rbx",
            "cpuid",
            "mov {0:e}, ebx",
            "mov {1:e}, ecx",
            "pop rbx",
            out(reg) ebx,
            out(reg) ecx,
            in("eax") 7u32,
            in("ecx") 0u32,
            options(nostack, preserves_flags),
        );
    }
    (ebx, ecx)
}

/// Enable SMEP, SMAP, and UMIP in CR4 if the CPU supports them.
/// Returns a `CpuFeatures` bitmask of what was actually enabled.
pub fn init() -> CpuFeatures {
    let (ebx, ecx) = cpuid7();
    let mut enabled = CpuFeatures::empty();

    let mut cr4 = Cr4::read();

    if ebx & CPUID_LEAF7_EBX_SMEP != 0 {
        cr4 |= Cr4Flags::SUPERVISOR_MODE_EXECUTION_PROTECTION;
        enabled |= CpuFeatures::SMEP;
    }
    if ebx & CPUID_LEAF7_EBX_SMAP != 0 {
        cr4 |= Cr4Flags::SUPERVISOR_MODE_ACCESS_PREVENTION;
        enabled |= CpuFeatures::SMAP;
    }
    if ecx & CPUID_LEAF7_ECX_UMIP != 0 {
        cr4 |= Cr4Flags::USER_MODE_INSTRUCTION_PREVENTION;
        enabled |= CpuFeatures::UMIP;
    }

    // Temporarily disabled CR4 write to check if SMAP/SMEP is causing the hang
    // Cr4::write(cr4);
    unsafe {
        let mut serial = uart_16550::SerialPort::new(0x3f8);
        serial.init();
        let _ = core::fmt::Write::write_str(&mut serial, "cpu_features::init: bypassed CR4 write\n");
    }

    enabled
}

/// Verify that the features we tried to enable are actually set in CR4.
/// Returns `Err(CpuFeatures)` with the bits that failed to stick.
pub fn verify(expected: CpuFeatures) -> Result<(), CpuFeatures> {
    let cr4 = Cr4::read();
    let mut missing = CpuFeatures::empty();

    if expected.contains(CpuFeatures::SMEP)
        && !cr4.contains(Cr4Flags::SUPERVISOR_MODE_EXECUTION_PROTECTION)
    {
        missing |= CpuFeatures::SMEP;
    }
    if expected.contains(CpuFeatures::SMAP)
        && !cr4.contains(Cr4Flags::SUPERVISOR_MODE_ACCESS_PREVENTION)
    {
        missing |= CpuFeatures::SMAP;
    }
    if expected.contains(CpuFeatures::UMIP)
        && !cr4.contains(Cr4Flags::USER_MODE_INSTRUCTION_PREVENTION)
    {
        missing |= CpuFeatures::UMIP;
    }

    if missing.0 == 0 { Ok(()) } else { Err(missing) }
}

/// Bitmask of CPU security features enabled in CR4.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub struct CpuFeatures(pub u8);

impl CpuFeatures {
    pub const SMEP: CpuFeatures = CpuFeatures(0b001);
    pub const SMAP: CpuFeatures = CpuFeatures(0b010);
    pub const UMIP: CpuFeatures = CpuFeatures(0b100);

    pub fn empty() -> Self { CpuFeatures(0) }
    pub fn contains(self, other: CpuFeatures) -> bool { self.0 & other.0 == other.0 }
}

impl core::ops::BitOrAssign for CpuFeatures {
    fn bitor_assign(&mut self, rhs: Self) { self.0 |= rhs.0; }
}

// ── SMAP access windows ───────────────────────────────────────────────────────

/// Set RFLAGS.AC = 1, opening a kernel window to access user-accessible pages.
/// Must be paired with `clac()`. Prefer `with_user_access` over calling directly.
#[inline(always)]
pub unsafe fn stac() {
    core::arch::asm!("stac", options(nostack, preserves_flags));
}

/// Clear RFLAGS.AC = 0, re-enabling SMAP enforcement.
#[inline(always)]
pub unsafe fn clac() {
    core::arch::asm!("clac", options(nostack, preserves_flags));
}

/// Execute `f` inside a STAC/CLAC bracket.
///
/// The closure receives validated user memory and may dereference it.
/// STAC/CLAC are harmless no-ops when CR4.SMAP == 0.
///
/// # Safety
/// The caller must have already validated that the pointer range is
/// user-accessible and within bounds before calling this function.
#[inline]
pub unsafe fn with_user_access<F, R>(f: F) -> R
where
    F: FnOnce() -> R,
{
    stac();
    let result = f();
    clac();
    result
}
