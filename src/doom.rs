/// DOOM Fire Effect — classic fire propagation algorithm
///
/// Uses the Zig blitter for the hot-path fire step and rendering.
/// Can render to a real framebuffer (if available) or output ASCII to serial.

use alloc::vec;
use alloc::vec::Vec;
use crate::println;

/// Fire dimensions (small enough to run fast, big enough to look good)
pub const FIRE_W: u32 = 80;
pub const FIRE_H: u32 = 50;

/// DOOM fire palette — 37 colors from black → red → orange → yellow → white
/// Each entry is XRGB8888 (0x00RRGGBB)
pub static DOOM_PALETTE: [u32; 37] = [
    0x00070707, // 0  — near black
    0x001F0707, // 1
    0x002F0F07, // 2
    0x00470F07, // 3
    0x00571707, // 4
    0x00671F07, // 5
    0x00772707, // 6
    0x007F2F07, // 7
    0x008F3707, // 8
    0x009F2F07, // 9
    0x00AF3F07, // 10
    0x00BF4707, // 11
    0x00C74707, // 12
    0x00DF4F07, // 13
    0x00DF5707, // 14
    0x00DF5707, // 15
    0x00D75F07, // 16
    0x00D7670F, // 17
    0x00CF6F0F, // 18
    0x00CF770F, // 19
    0x00CF7F0F, // 20
    0x00CF8717, // 21
    0x00C78717, // 22
    0x00C78F17, // 23
    0x00C7971F, // 24
    0x00BF9F1F, // 25
    0x00BF9F1F, // 26
    0x00BFA727, // 27
    0x00BFAF2F, // 28
    0x00B7AF2F, // 29
    0x00B7B72F, // 30
    0x00B7B737, // 31
    0x00CFCF6F, // 32
    0x00DFDF9F, // 33
    0x00EFEFC7, // 34
    0x00FFFFFF, // 35 — white
    0x00FFFFFF, // 36 — white (overflow guard)
];

/// Initialize the fire buffer: all zeros except the bottom row = max intensity.
pub fn init_fire_buf(buf: &mut [u8], w: u32, h: u32) {
    // Clear everything
    for b in buf.iter_mut() {
        *b = 0;
    }
    // Set bottom row to max fire (palette index 36)
    let bottom_start = (h - 1) * w;
    for x in 0..w {
        buf[(bottom_start + x) as usize] = 36;
    }
}

/// Run the DOOM fire demo for `steps` iterations, outputting ASCII to serial.
/// This works even in headless mode (-display none).
pub fn run_serial(steps: usize) {
    println!("\n━━━ DOOM Fire (Zig-powered, {} steps) ━━━", steps);

    let buf_size = (FIRE_W * FIRE_H) as usize;
    let mut fire_buf: Vec<u8> = vec![0u8; buf_size];
    init_fire_buf(&mut fire_buf, FIRE_W, FIRE_H);

    // We need a dummy framebuffer for the fire step (it writes pixels there)
    // but for serial mode we only care about the fire_buf after stepping.
    // We'll allocate a small one just for the algorithm.
    let fb_pitch = FIRE_W * 4;
    let fb_size = (fb_pitch * FIRE_H) as usize;
    let mut dummy_fb: Vec<u8> = vec![0u8; fb_size];

    // ASCII output buffer
    let ascii_buf_size = ((FIRE_W + 1) * (FIRE_H / 2) + 16) as usize;
    let mut ascii_buf: Vec<u8> = vec![0u8; ascii_buf_size];

    for step in 0..steps {
        // Run the Zig fire propagation + render
        crate::zig_ffi::doom_fire_step(
            dummy_fb.as_mut_ptr(),
            fb_pitch,
            FIRE_W,
            FIRE_H,
            &DOOM_PALETTE,
            &mut fire_buf,
        );

        // Only print every Nth frame to serial (too slow otherwise)
        if step % 4 == 0 || step == steps - 1 {
            let written = crate::zig_ffi::doom_fire_to_ascii(
                &fire_buf, FIRE_W, FIRE_H, &mut ascii_buf,
            );
            if written > 0 {
                // Print the ASCII frame
                if let Ok(s) = core::str::from_utf8(&ascii_buf[..written]) {
                    // Clear-ish: print separator then the frame
                    println!("── frame {} ──", step);
                    crate::print!("{}", s);
                }
            }
        }
    }
    println!("━━━ DOOM Fire complete ({} steps) ━━━", steps);
}

/// Run the fire demo with framebuffer rendering (when a real FB is available).
/// Falls back to serial ASCII if no framebuffer is initialized.
pub fn run(steps: usize) {
    // For now, always use serial mode since VGA text mode is active.
    // When DRM/framebuffer is properly mapped, this can render to real pixels.
    run_serial(steps);
}
