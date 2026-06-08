/// RISC-V 64 interrupt controller drivers
/// Ported from Redox OS.

pub mod hlic;
pub mod plic;

pub use hlic::Hlic;
pub use plic::Plic;
