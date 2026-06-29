//! ZiqaKernel IPC compatibility layer for Orbital
//!
//! This module provides a bridge between Orbital's expected IPC interface
//! and ZiqaKernel's channel-based IPC system.

use std::sync::Arc;

/// Re-export the kernel's IPC types for compatibility
pub use ziqa_kernel::ipc::{Channel, Message, IpcError};

/// Channel ID type alias
pub type ChannelId = u32;

/// Create a new IPC channel
pub fn create_channel() -> Result<ChannelId, IpcError> {
    ziqa_kernel::ipc::create_channel().ok_or(IpcError::InvalidChannel)
}

/// Get an existing channel
pub fn get_channel(id: ChannelId) -> Result<Arc<Channel>, IpcError> {
    ziqa_kernel::ipc::get_channel(id as usize)
}

/// Send a message on a channel
pub fn send(channel_id: ChannelId, data: &[u8]) -> Result<(), IpcError> {
    // For now, we use the global channel table
    // In a full implementation, you'd want to track the sender PID
    let pid = ziqa_kernel::arch::current_pid().unwrap_or(ziqa_kernel::process::Pid(0));
    ziqa_kernel::ipc::send(channel_id, pid, data)
}

/// Receive a message from a channel
pub fn recv(channel_id: ChannelId) -> Result<Message, IpcError> {
    ziqa_kernel::ipc::recv(channel_id)
}

/// Initialize the IPC system for Orbital
pub fn init() -> Result<(), String> {
    // The kernel IPC is already initialized by the kernel
    Ok(())
}