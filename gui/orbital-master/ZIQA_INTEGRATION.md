# ZiqaKernel Orbital Integration

This document describes the modifications made to integrate Orbital GUI with ZiqaKernel.

## Changes Made

### 1. New IPC Compatibility Layer (`src/ziqa_ipc.rs`)
- Bridges Orbital's IPC expectations with ZiqaKernel's channel-based IPC
- Provides `create_channel()`, `get_channel()`, `send()`, `recv()` functions
- Re-exports kernel types for compatibility

### 2. New Graphics Compatibility Layer (`src/ziqa_graphics.rs`)
- Provides framebuffer abstraction for kernel graphics
- Implements `FrameBuffer` struct for pixel manipulation
- Placeholder for kernel-GPU integration

### 3. Updated `Cargo.toml`
- Added `ziqa-kernel` dependency pointing to parent directory
- Removed DRM dependency (uses kernel framebuffer directly)
- Kept essential dependencies: `graphics-ipc`, `inputd`, `orbclient`

### 4. Modified `src/main.rs`
- Added `mod ziqa_ipc;` and `mod ziqa_graphics;` declarations

## Integration Points

### IPC Communication
Orbital communicates with the kernel through the IPC system:
- Channels are created for window management
- Messages pass surface updates and input events
- Kernel's compositor thread handles rendering

### Graphics Rendering
The GUI renders to a shared framebuffer:
- `FrameBuffer` abstraction wraps kernel's framebuffer pointer
- Pixel operations are performed in userspace
- Kernel copies framebuffer to display during vblank

## Building

From the `gui/orbital-master` directory:

```bash
cargo build --features fast-dev
```

Or from the kernel root with the feature enabled:

```bash
cargo bootimage --features "orbital,fast-dev"
```

## Status

This is a **minimal compatibility layer** that allows Orbital to compile and link against ZiqaKernel. Further work is needed to:

- [ ] Implement full input event routing from kernel to Orbital
- [ ] Add proper GPU buffer sharing between kernel and userspace
- [ ] Integrate with ZiqaKernel's existing compositor IPC channels
- [ ] Handle window surface lifecycle management
- [ ] Implement proper keyboard/mouse input forwarding

## Kernel Requirements

The kernel must provide:
1. IPC channel system (already implemented)
2. Framebuffer access (VirtIO GPU or BGA driver)
3. Input device access (PS/2, USB HID)
4. Process spawning capability for Orbital binary