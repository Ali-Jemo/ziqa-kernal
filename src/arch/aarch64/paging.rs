/// AArch64 paging initialization (MAIR, etc.)
/// Ported from Redox OS.

pub fn init() {
    unsafe {
        rmm::aarch64::init_mair();
    }
}
