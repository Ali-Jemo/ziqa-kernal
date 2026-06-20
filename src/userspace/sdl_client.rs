//! Minimal SDL-style userspace client.
//!
//! Uses ZiqaKernel kernel services directly (same as other built-in
//! demo clients) to render a test window on top of the `sdl:` scheme path.
//!
//! ponytail: single test client first; real SDL shim only after the
//! Linux ABI/ELF loader path is extended to map `bin/*` automatically.

use crate::abi::syscall::{KERNEL_SYSCALL_OPEN, KERNEL_SYSCALL_READ, KERNEL_SYSCALL_WRITE, syscall};
use crate::process::Pid;

const SDL_INFO: &str = "sdl:info";
const SDL_EVENTS: &str = "sdl:events";
const SDL_FRAME: &str = "sdl:frame";

const EVENT_HEADER_LEN: usize = 16;
const INFO_HEADER_LEN: usize = 16;

#[repr(C)]
struct SdlEvent {
    kind: u32,
    a: u32,
    b: u32,
    c: u32,
}

fn kopen(path: &str, flags: usize) -> Option<i32> {
    syscall(KERNEL_SYSCALL_OPEN, args(path, flags, 0, 0, 0, 0)).ok().map(|v| v as i32)
}
fn kread(fd: i32, buf: &mut [u8]) -> bool {
    syscall(KERNEL_SYSCALL_READ, args_tail(usize::from(fd), buf.as_ptr() as _, buf.len(), 0, 0, 0))
        .ok()
        .map(|v| v > 0)
        .unwrap_or(false)
}
fn kwrite(fd: i32, buf: &[u8]) -> Option<usize> {
    syscall(KERNEL_SYSCALL_WRITE, args_tail(usize::from(fd), buf.as_ptr() as _, buf.len(), 0, 0, 0))
        .ok()
        .map(|v| v as usize)
}

const fn args(a: usize, b: usize, c: usize, d: usize, e: usize, f: usize) -> [usize; 6] {
    [a, b, c, d, e, f]
}
const fn args_tail(a: usize, b: usize, c: usize, d: usize, e: usize, f: usize) -> [usize; 6] {
    [a, b, c, d, e, f]
}

pub fn sdl_client_main(_arg: *const ()) {
    for _ in 0..40000 {
        core::hint::spin_loop();
    }

    let Some(info_fd) = kopen(SDL_INFO, 0) else {
        crate::klog!(crate::klog::Level::Error, "[SDLClient] open sdl:info failed");
        return;
    };
    let Some(events_fd) = kopen(SDL_EVENTS, 0) else {
        crate::klog!(crate::klog::Level::Error, "[SDLClient] open sdl:events failed");
        return;
    };
    let Some(frame_fd) = kopen(SDL_FRAME, 1) else {
        crate::klog!(crate::klog::Level::Error, "[SDLClient] open sdl:frame failed");
        return;
    };

    let mut info_buf = [0u8; INFO_HEADER_LEN];
    if !kread(info_fd, &mut info_buf) {
        crate::klog!(crate::klog::Level::Error, "[SDLClient] read info failed");
        return;
    }
    let dst_w = u32::from_le_bytes([info_buf[4], info_buf[5], info_buf[6], info_buf[7]]) as usize;
    let dst_h = u32::from_le_bytes([info_buf[8], info_buf[9], info_buf[10], info_buf[11]]) as usize;
    crate::klog!(crate::klog::Level::Info, "[SDLClient] display {}x{}", dst_w, dst_h);

    let width: usize = 320;
    let height: usize = 240;
    let mut pixels = [0u32; 320 * 240];
    let mut tick = 0u32;

    loop {
        let mut ev_buf = [0u8; EVENT_HEADER_LEN];
        let _ = kread(events_fd, &mut ev_buf);
        let ev = unsafe { &*(ev_buf.as_ptr() as *const SdlEvent) };

        for py in 0..height {
            for px in 0..width {
                let r = ((px + tick) & 0xFF) as u32;
                let g = ((py + tick) & 0xFF) as u32;
                let b = ((px ^ py).wrapping_add(tick) & 0xFF) as u32;
                pixels[py * width + px] = (r << 16) | (g << 8) | b | 0xFF000000;
            }
        }
        tick = tick.wrapping_add(1);

        let frame_w = width.min(dst_w);
        let frame_h = height.min(dst_h);
        let mut frame = alloc::vec![0u8; 16 + frame_w * frame_h * 4];
        frame[0..4].copy_from_slice(b"ZSDL");
        (frame_w as u32).to_le_bytes().clone_into(&mut frame[4..8]);
        (frame_h as u32).to_le_bytes().clone_into(&mut frame[8..12]);
        ((frame_w * 4) as u32).to_le_bytes().clone_into(&mut frame[12..16]);
        for row in 0..frame_h {
            let src = &pixels[row * width..row * width + frame_w];
            let dst = &mut frame[16 + row * (frame_w * 4)..16 + (row + 1) * (frame_w * 4)];
            for (i, px) in src.iter().enumerate() {
                dst[i * 4..i * 4 + 4].copy_from_slice(&px.to_le_bytes());
            }
        }

        let _ = kwrite(frame_fd, &frame);
        crate::process::scheduler::yield_now();
    }
}
