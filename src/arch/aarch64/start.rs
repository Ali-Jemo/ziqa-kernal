//! AArch64 kernel start and initialization
//! Based on Redox OS implementation.

use core::{arch::naked_asm, cell::SyncUnsafeCell, slice};

use crate::{
    allocator,
    arch::{device, paging},
    startup::memory,
};

/// Test of zero values in BSS.
static mut BSS_TEST_ZERO: usize = 0;
/// Test of non-zero values in data.
static mut DATA_TEST_NONZERO: usize = 0xFFFF_FFFF_FFFF_FFFF;

#[repr(C, align(16))]
struct StackAlign<T>(T);

static STACK: SyncUnsafeCell<StackAlign<[u8; 128 * 1024]>> =
    SyncUnsafeCell::new(StackAlign([0; 128 * 1024]));

/// Entry point from bootloader (naked assembly)
#[unsafe(naked)]
#[unsafe(no_mangle)]
extern "C" fn kstart() {
    naked_asm!(
        "
        // BSS should already be zero
        adrp x9, {bss_test_zero}
        ldr x9, [x9, :lo12:{bss_test_zero}]
        cbnz x9, .Lkstart_crash
        adrp x9, {data_test_nonzero}
        ldr x9, [x9, :lo12:{data_test_nonzero}]
        cbz x9, .Lkstart_crash

        adrp x1, {stack}
        add x1, x1, :lo12:{stack}
        mov x2, {stack_size}-16
        add sp, x1, x2

        // Setup interrupt handlers
        ldr x9, =exception_vector_base
        msr vbar_el1, x9

        mov lr, 0
        b {start}

    .Lkstart_crash:
        mov x9, 0
        br x9
        ",
        bss_test_zero = sym BSS_TEST_ZERO,
        data_test_nonzero = sym DATA_TEST_NONZERO,
        stack = sym STACK,
        stack_size = const size_of_val(&STACK),
        start = sym start,
    );
}

/// The entry to Rust, all things must be initialized
/// This is called from kstart after basic setup
#[unsafe(no_mangle)]
pub unsafe extern "C" fn start(args_ptr: *const crate::startup::KernelArgs) -> ! {
    unsafe {
        let args = args_ptr.read();

        // Initialize RMM
        crate::startup::memory::init(&args, None, None);

        // Initialize paging
        paging::init();

        // Initialize per-CPU data
        crate::arch::misc::init(0);

        // Setup kernel heap
        allocator::init();

        // Initialize devices via device tree if available
        // This will be expanded when DTB support is added

        // Call into common kernel main
        crate::init::init_abi_registry();
        
        // For now, just halt
        loop {
            core::arch::asm!("wfe");
        }
    }
}

/// Entry to rust for an AP (Application Processor)
#[allow(unused)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kstart_ap(_args_ptr: *const crate::startup::KernelArgsAp) -> ! {
    loop {
        core::arch::asm!("wfe");
    }
}

#[repr(C, packed)]
#[allow(unused)]
pub struct KernelArgsAp;