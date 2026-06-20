#![no_std]
#![no_main]

use ziqa_kernel::println;

#[no_mangle]
pub extern "C" fn _start() -> ! {
    println!("[Orbital Test Client] Requesting desktop render...");
    // This client would now communicate with the compositor to request UI elements
    loop {}
}
