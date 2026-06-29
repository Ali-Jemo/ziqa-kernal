//! Built-in demo client for the compositor.
//!
//! Runs as a kernel thread, connects to the compositor over IPC,
//! and renders an animated gradient surface.

use crate::ipc::gui::*;
use crate::process::Pid;

const DEMO_MAX_W: u32 = 1280;
const DEMO_MAX_H: u32 = 960;
const DEMO_MIN_W: u32 = 320;
const DEMO_MIN_H: u32 = 240;
const DEMO_FRAME_MS: u64 = 16;
const DEMO_SURFACE_ID: u32 = 1;

const fn scaled_extent(available: u32, max: u32, min: u32) -> u32 {
    let target = available.saturating_mul(3) / 4;
    if available < min {
        available
    } else {
        let capped = if target < max { target } else { max };
        if capped > min {
            capped
        } else {
            min
        }
    }
}

const fn demo_surface_geometry(fb_w: u32, fb_h: u32) -> (u32, u32, i32, i32) {
    let w = scaled_extent(fb_w, DEMO_MAX_W, DEMO_MIN_W);
    let h = scaled_extent(fb_h, DEMO_MAX_H, DEMO_MIN_H);
    let x = (fb_w.saturating_sub(w) / 2) as i32;
    let y = (fb_h.saturating_sub(h) / 2) as i32;
    (w, h, x, y)
}

fn framebuffer_size() -> (u32, u32) {
    crate::drivers::virtio_gpu::get_fb_info()
        .or_else(crate::drivers::framebuffer::get_bga_fb_info)
        .map(|(_, w, h, _)| (w, h))
        .unwrap_or((1024, 768))
}
const COMPOSITOR_CHAN: u32 = 3;
unsafe fn any_as_bytes<T: Sized>(x: &T) -> &[u8] {
    unsafe { core::slice::from_raw_parts(x as *const T as *const u8, core::mem::size_of::<T>()) }
}

/// Send a message to the compositor channel.
fn send_msg(opcode: u8, payload: &[u8]) {
    let mut buf = [0u8; 256];
    buf[0] = opcode;
    if !payload.is_empty() {
        let n = payload.len().min(255);
        buf[1..1 + n].copy_from_slice(&payload[..n]);
    }
    let _ = crate::ipc::send(COMPOSITOR_CHAN, Pid(0), &buf[..1 + payload.len().min(255)]);
}

/// Kernel-thread entry for the built-in demo client.
pub fn demo_client_main(_arg: *const ()) {
    let pid = crate::process::scheduler::SCHEDULER.current_pid();
    if let Some(pid) = pid {
        crate::timer::sleep_ms(pid, 1000);
        crate::process::scheduler::yield_now();
    }

    let (fb_w, fb_h) = framebuffer_size();
    let (width, height, pos_x, pos_y) = demo_surface_geometry(fb_w, fb_h);
    // 1. Create SHM via the kernel SHM module
    let shm_id = match crate::ipc::shm::SHM
        .lock()
        .create(Pid(0), (width * height * 4) as usize)
    {
        Ok(id) => id,
        Err(_) => {
            crate::klog!(crate::klog::Level::Error, "[DemoClient] SHM create failed");
            return;
        }
    };

    let shm_addr = match crate::ipc::shm::SHM.lock().attach(shm_id, Pid(0)) {
        Ok(addr) => addr,
        Err(_) => {
            crate::klog!(crate::klog::Level::Error, "[DemoClient] SHM attach failed");
            return;
        }
    };

    crate::klog!(
        crate::klog::Level::Info,
        "[DemoClient] SHM created id={} addr=0x{:x} ({}x{} = {} bytes)",
        shm_id,
        shm_addr,
        width,
        height,
        width * height * 4,
    );

    // 2. Connect (OpCode 1)
    let conn = ConnectMsg { pid: 0 };
    send_msg(OpCode::Connect as u8, unsafe { any_as_bytes(&conn) });

    // 3. Create surface (OpCode 2)
    let surf = CreateSurfaceMsg { surface_id: DEMO_SURFACE_ID, width, height };
    send_msg(OpCode::CreateSurface as u8, unsafe { any_as_bytes(&surf) });

    // 4. BufferAttach
    let attach = BufferAttachMsg {
        surface_id: DEMO_SURFACE_ID,
        shm_id,
        width,
        height,
    };
    send_msg(OpCode::BufferAttach as u8, unsafe { any_as_bytes(&attach) });

    // 5. Set position
    let pos = SetPositionMsg {
        surface_id: DEMO_SURFACE_ID,
        x: pos_x,
        y: pos_y,
    };
    send_msg(OpCode::SetPosition as u8, unsafe { any_as_bytes(&pos) });

    let shm_ptr = shm_addr as *mut u32;
    let mut tick: u32 = 0;

    // 6. Render loop
    loop {
        tick = tick.wrapping_add(1);

        for y in 0..height {
            for x in 0..width {
                let r = (x.wrapping_add(tick)) & 0xFF;
                let g = (y.wrapping_add(tick)) & 0xFF;
                let b = (x.wrapping_add(y).wrapping_add(tick)) & 0xFF;
                unsafe {
                    let idx = (y as usize) * (width as usize) + (x as usize);
                    shm_ptr
                        .add(idx)
                        .write_volatile((r << 16) | (g << 8) | b | 0xFF000000);
                }
            }
        }

        let flush = FlushMsg {
            surface_id: DEMO_SURFACE_ID,
            x: 0,
            y: 0,
            width,
            height,
        };
        send_msg(OpCode::Flush as u8, unsafe { any_as_bytes(&flush) });

        if let Some(pid) = pid {
            crate::timer::sleep_ms(pid, DEMO_FRAME_MS);
        }
        crate::process::scheduler::yield_now();
    }
}

const _: () = {
    let bga = demo_surface_geometry(1024, 768);
    assert!(bga.0 == 768 && bga.1 == 576 && bga.2 == 128 && bga.3 == 96);

    let small = demo_surface_geometry(320, 200);
    assert!(small.0 == 320 && small.1 == 200 && small.2 == 0 && small.3 == 0);
};
