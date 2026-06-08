/// AArch64 interrupt controller drivers (GIC)
/// Ported from Redox OS.

mod gic;

pub use gic::Gic;
