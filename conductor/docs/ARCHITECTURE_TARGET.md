# ZiqaKernel Build Configuration

This document explains the configuration files used to build and run ZiqaKernel as a bare-metal OS for x86_64.

## Target Specification (`x86_64-unknown-none.json`)

The target JSON file defines the Rust target for our kernel. It tells `rustc` how to compile code for our specific bare-metal environment.

### Fields

- **arch**: `x86_64` - The processor architecture.
- **code-model**: `kernel` - The kernel code model, which affects how addresses are handled.
- **cpu**: `x86-64` - The specific CPU target.
- **crt-objects-fallback**: `false` - Do not use the C runtime objects fallback.
- **data-layout**: `e-m:e-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-f80:128-n8:16:32:64-S128` - The data layout string describing memory layout, endianness, pointer width, etc.
- **disable-redzone**: `true` - Disable the red zone (a 128-byte area below the stack pointer that is not modified by signal or interrupt handlers) because we are in an OS kernel where interrupts can occur at any time.
- **features**: `-mmx,-sse,-sse2,-sse3,-ssse3,-sse4.1,-sse4.2,-avx,-avx2,+soft-float` - Disable SIMD features and enable soft-float because we are in a bare-metal environment without floating-point unit support in the kernel (initially).
- **linker**: `rust-lld` - Use the Rust LLD linker.
- **linker-flavor**: `gnu-lld` - The linker flavor is GNU-compatible.
- **llvm-target**: `x86_64-unknown-none-elf` - The LLVM target triple.
- **max-atomic-width**: `64` - Maximum width for atomic operations is 64 bits.
- **metadata**: Contains metadata about the target (description, host_tools, std, tier).
- **panic-strategy**: `abort` - On panic, abort the kernel (since we cannot unwind in bare-metal easily).
- **plt-by-default**: `false` - Do not use Procedure Linkage Table by default (not needed in kernel).
- **position-independent-executables**: `false` - The kernel is not position-independent; it is loaded at a fixed address.
- **relro-level**: `full` - Set RELRO (Read-Only Relocations) to full for security (though effectiveness in kernel is limited).
- **rustc-abi**: `softfloat` - Use software floating-point ABI.
- **stack-probes**: `{ "kind": "inline" }` - Use inline stack probes to check for stack overflow.
- **static-position-independent-executables**: `false` - Not applicable.
- **supported-sanitizers**: `[ "kcfi", "kernel-address" ]` - Supported sanitizers: Kernel Control Flow Integrity and Kernel Address Sanitizer.
- **target-pointer-width**: `64` - Pointers are 64 bits wide.

## Linker Script (`linker.ld`)

The linker script defines how sections of the kernel are placed in memory.

### Sections

- **ENTRY(_start)** - The entry point of the kernel is the symbol `_start`.
- **Memory layout starts at 0x100000 (1 MiB)** - This is a common convention for kernels to leave space for BIOS/bootloader data.
- **.text** - Contains executable code.
- **.rodata** - Read-only data (constants).
- **.data** - Initialized global and static variables.
- **.bss** - Uninitialized global and static variables (zero-initialized).

## Bootimage Configuration (`bootimage.toml`)

This file configures the `bootimage` tool, which creates a bootable disk image from the kernel binary.

### Settings

- **binary**: `"ziqa-kernel"` - The name of the kernel binary to package.
- **runner**: The command to run the boot image in QEMU, including:
  - `-drive` for the boot image and disk image.
  - `-serial` for stdio serial output.
  - `-display none` to disable graphical output.
  - `-device virtio-net-pci` for network device.
  - `-netdev user` for user-mode networking.
- **target**: `"x86_64-unknown-none"` - The Rust target to use for building.

## Rust Toolchain (`rust-toolchain.toml`)

Specifies the Rust toolchain used for development.

- **channel**: `"nightly"` - We use the nightly channel for unstable features needed in OS development.
- **components**: `[ "rust-src", "llvm-tools-preview" ]` - Rust source (for OS development) and LLVM tools (for tools like `llvm-objdump`).
- **targets**: `[ "x86_64-unknown-none" ]` - We only build for our custom target.

## How It All Comes Together

When you run `make` or `cargo build`, the following happens:

1. `rustc` compiles the kernel source using the target specification (`x86_64-unknown-none.json`).
2. The linker uses `linker.ld` to place sections in the binary.
3. The resulting binary is passed to `bootimage` which, according to `bootimage.toml`, creates a bootable disk image.
4. The disk image can be run with QEMU using the command specified in `bootimage.toml`.

This configuration ensures that the kernel is built as a bare-metal ELF executable that can be loaded by BIOS/UEFI (via the bootimage tool) and run in a QEMU virtual machine.

## Build Script (`build.rs`)

The Rust build script (`build.rs`) runs before the kernel is compiled. It handles:

- Building the Zig code that is linked into the kernel
- Setting up environment for the bootimage tool
- Generating or processing any build-time configuration

### Key Functions

- **main()**: Entry point for the build script.
- **build()**: Build function for Zig integration.

## Zig Build (`build.zig`)

The Zig build file defines how Zig components are compiled and linked into the kernel. The `build.zig` typically includes:

- **build()**: The build function that defines how Zig source files are compiled as object files and linked with the Rust kernel.

## Cargo Configuration (`Cargo.toml`)

The `Cargo.toml` defines the Rust project structure:

- **Package name**: `my_os` (the kernel crate)
- **Edition**: 2021
- **Dependencies**: Includes `bootloader`, `x86_64`, `uart_16550`, `lazy_static`, `spin`, `pic8259`, `pc-keyboard`, `linked_list_allocator`, and others.
- **Profile**: Custom release profile for kernel binary size optimization.
- **Dependencies**: Various crates for OS development including:
  - `bootloader` - for creating bootable kernel images
  - `x86_64` - x86_64 architecture abstractions
  - `uart_16550` - serial port I/O
  - `lazy_static` - lazy initialization statics
  - `spin` - spinlock synchronization
  - `pic8259` - interrupt controller
  - `acpi` - ACPI table parsing
  - `alloc` - heap allocation support


## Graphics & Compositor Architecture

- **Kernel-mode Compositor**: Implemented in `src/userspace/compositor.rs`, this thread manages display surfaces, tracks dirty regions, and composites only after a working VirtIO GPU framebuffer is available.
- **BGA Fallback**: The display system falls back to the Bochs Graphics Adapter (BGA) if VirtIO GPU framebuffer setup is unavailable. In that mode the framebuffer console remains active, so QEMU GTK continues mirroring shell output instead of handing the display to the compositor.
- **Client Protocol**: IPC-based protocol (channel 3) for clients to create surfaces, attach SHM buffers, set positions, and flush updates.
- **Input Forwarding**: The foreground shell owns serial/PS/2 input while line editing. Compositor clients receive input events through the compositor input channel when the compositor display path is active.

## Makefile (`Makefile`)

The `Makefile` provides convenience commands for building and running the kernel:

- **make**: Build the kernel
- **make run**: Run the kernel in QEMU
- **make run-gui**: Run QEMU with a GTK display and a raw terminal serial shell. The runner saves/restores host terminal state and uses `-serial stdio` for byte-at-a-time shell input.
- **make clean**: Clean build artifacts
- **Other targets**: Build and test commands

## Docker Configuration

### `Dockerfile`
The Dockerfile sets up the development environment with all necessary tools:
- Rust toolchain (nightly)
- QEMU for running the kernel
- Essential build tools and libraries

### `docker-compose.yml`
Docker Compose configuration for easy development setup:
- **dev service**: The development container with the kernel source mounted
- Environment configuration for cross-compilation
- Volume mounts for caching and build artifacts
