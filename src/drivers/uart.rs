use lazy_static::lazy_static;
use spin::Mutex;
use uart_16550::SerialPort;

lazy_static! {
    /// Graph: used_by drivers::vga, drivers::ata, kernel_main
    pub static ref SERIAL1: Mutex<SerialPort> = {
        let mut serial_port = unsafe { SerialPort::new(0x3F8) };
        serial_port.init();
        Mutex::new(serial_port)
    };
}

pub static VGA_ENABLED: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);

struct SerialWrapper<'a>(&'a mut SerialPort);

impl<'a> core::fmt::Write for SerialWrapper<'a> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        for b in s.bytes() {
            if b == b'\n' {
                self.0.send(b'\r');
            }
            self.0.send(b);
        }
        Ok(())
    }
}

#[doc(hidden)]
pub fn _print(args: ::core::fmt::Arguments) {
    use core::fmt::Write;
    use x86_64::instructions::interrupts;

    // Disable interrupts while holding locks to prevent deadlocks.
    // Serial + VGA text + framebuffer console are all acquired here.
    interrupts::without_interrupts(|| {
        let mut serial = SERIAL1.lock();
        let mut wrapper = SerialWrapper(&mut serial);
        wrapper.write_fmt(args).expect("Printing to serial failed");

        if VGA_ENABLED.load(core::sync::atomic::Ordering::Relaxed) {
            crate::drivers::vga::WRITER.lock().write_fmt(args).unwrap();
        }

        // Render to the GPU / BGA framebuffer console when active.
        // During early boot no compositor claims the display yet, so the
        // shell output is visible on the QEMU window.  Once Orbital or the
        // native compositor starts it will clear the buffer and draw its
        // own content – occasional stray shell lines overwriting it are
        // harmless (and brief).
        if crate::drivers::fb_console::GPU_CONSOLE_ACTIVE.load(core::sync::atomic::Ordering::Relaxed) {
            let mut fb = crate::drivers::fb_console::FB_CONSOLE.lock();
            if let Some(ref mut console) = *fb {
                let _ = console.write_fmt(args);
            }
        }
    });
}

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => {
        $crate::drivers::uart::_print(format_args!($($arg)*))
    };
}

#[macro_export]
macro_rules! println {
    () => ($crate::print!("\n"));
    ($fmt:expr) => ($crate::print!(concat!($fmt, "\n")));
    ($fmt:expr, $($arg:tt)*) => ($crate::print!(concat!($fmt, "\n"), $($arg)*));
}
