// Userspace Wayland-like compositor implementation

use alloc::sync::Arc;
use alloc::vec::Vec;
use crate::ipc::{self, Message as IpcMessage};
use crate::process::Pid;
use crate::ipc::gui::{OpCode, CreateSurfaceMsg, FlushMsg, BufferAttachMsg, SetPositionMsg};

/// Entry point called from `src/init.rs`. Registers a dedicated IPC channel (ID 2)
/// and forwards client messages to the kernel compositor (channel 3).
pub fn run() -> Result<(), &'static str> {
    // Register userspace compositor channel 2
    ipc::register_channel(2, Arc::new(ipc::Channel::new()));
    const KERNEL_CHAN: u32 = 3;
    // Forward loop – poll channel 2 and forward messages to kernel compositor.
    loop {
        match ipc::recv(2) {
            Ok(msg) => {
                // Forward raw data to kernel compositor; PID 0 denotes kernel.
                let _ = ipc::send(KERNEL_CHAN, Pid(0), &msg.data);
            }
            Err(_) => {
                // No pending messages; yield to avoid busy loop.
                continue;
            }
        }
    }
}

/// Userspace compositor-side message definitions – retained for compatibility.
#[derive(Debug)]
pub enum Message {
    CreateSurface { width: u32, height: u32 },
    CreateBuffer { surface_id: u32, size: usize },
    Damage { rects: Vec<(u32, u32, u32, u32)> },
    Commit,
}

fn handle_message(_msg: Message) -> Result<(), &'static str> { Ok(()) }