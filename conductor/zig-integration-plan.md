# Implementation Plan: Zig Integration and Doom Port

## Objective
Accelerate the kernel build process, enhance kernel performance with Zig, and successfully port and run Doom as a user-space ELF process within the kernel.

## 1. Unified Build System (`build.zig`)
We will replace the current `Makefile` with a high-performance `build.zig` script. 
* **Rust Orchestration:** The `build.zig` script will use `std.process.Child` to invoke `cargo build --release` and `cargo bootimage`.
* **Zig Compilation:** It will handle compiling Zig and C code (like Doom) natively without needing a separate C compiler, leveraging Zig's built-in `cc`.
* **QEMU Runner:** A `zig build run` step will execute QEMU.
* **Benefit:** Zig's build system will provide aggressive caching, fast incremental builds, and a unified toolchain.

## 2. Kernel Feature in Zig: Fast Framebuffer Blitter
To demonstrate Zig's kernel-level performance (especially useful for Doom), we will implement a fast memory/framebuffer blitter in Zig.
* **Component:** `src/zig/blitter.zig`.
* **Integration:** Compile `blitter.zig` into a static library (`libblitter.a`) using `zig build`. Create a `build.rs` for the Rust kernel to link against this static library.
* **Usage:** Rust will call `extern "C" fn zig_fb_flush(...)` to rapidly copy pixel buffers to the screen.

## 3. Porting Doom (`doomgeneric`)
* **Source:** We will use a C-based Doom port (like `doomgeneric`) and compile it using Zig's cross-compiler targeting `x86_64-freestanding` (ELF).
* **System Calls:** We will write a small Zig wrapper (`doom_port.zig`) that implements the required Doom functions (`DG_DrawFrame`, `DG_SleepMs`, `DG_GetTicks`, `DG_GetKey`) by making `sys_*` system calls to our Rust kernel (e.g., using `asm` blocks in Zig to trigger interrupts or syscalls).
* **Output:** A self-contained `doom.elf` binary.

## 4. Running Doom
* **Embedding:** Place `doom.elf` in the `assets/` directory.
* **VFS Mount:** In `src/main.rs`, use `include_bytes!("../assets/doom.elf")` and mount it to `/bin/doom` in the `RamFS`.
* **Execution:** Add logic to spawn a new process executing `/bin/doom` using the existing `elf_loader`.
* **Kernel Syscalls:** Ensure the Rust kernel implements the necessary system calls (time, keyboard input, framebuffer draw) required by the Doom Zig wrapper.

## Verification & Testing
* `zig build` compiles the kernel and Doom seamlessly.
* QEMU boots without panic.
* The kernel spawns the Doom ELF.
* Doom successfully writes pixels to the framebuffer using the Zig blitter and receives keyboard input.