/// RISC-V 64 CPU feature detection
/// Ported from Redox OS.

use core::fmt::{Result, Write};

pub fn cpu_info<W: Write>(_w: &mut W) -> Result {
    Ok(())
}
