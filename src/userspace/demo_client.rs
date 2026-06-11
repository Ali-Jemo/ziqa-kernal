//! Built-in demo client for the compositor.
//!
//! Runs as a kernel thread, connects to the compositor over IPC,
//! and renders an animated gradient surface.

use crate::ipc::gui::*;
use crate::process::Pid;

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
    for _ in 0..100_000 {
        core::hint::spin_loop();
    }

    let width: u32 = 320;
    let height: u32 = 240;

    // 1. Create SHM via the kernel SHM module
    let shm_id = match crate::ipc::shm::SHM.lock().create(Pid(0), (width * height * 4) as usize) {
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
        shm_id, shm_addr, width, height, width * height * 4,
    );

    // 2. Connect (OpCode 1)
    let conn = ConnectMsg { pid: 0 };
    send_msg(
        OpCode::Connect as u8,
        unsafe { any_as_bytes(&conn) },
    );

    // 3. Create surface (OpCode 2)
    let surf = CreateSurfaceMsg { width, height };
    send_msg(
        OpCode::CreateSurface as u8,
        unsafe { any_as_bytes(&surf) },
    );

    // 4. BufferAttach
    let attach = BufferAttachMsg { surface_id: 1, shm_id, width, height };
    send_msg(
        OpCode::BufferAttach as u8,
        unsafe { any_as_bytes(&attach) },
    );

    // 5. Set position
    let pos = SetPositionMsg { surface_id: 1, x: 352, y: 264 };
    send_msg(
        OpCode::SetPosition as u8,
        unsafe { any_as_bytes(&pos) },
    );

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
                    shm_ptr.add(idx).write_volatile((r << 16) | (g << 8) | b | 0xFF000000);
                }
            }
        }

        let flush = FlushMsg { surface_id: 1, x: 0, y: 0, width, height };
        send_msg(
            OpCode::Flush as u8,
            unsafe { any_as_bytes(&flush) },
        );

        crate::process::scheduler::yield_now();
    }
}
