# Makefile for ZiqaKernel - Fast build system

SHELL := /bin/zsh

.PHONY: build run run-gui run-headless run-vnc clean test profile fat-disk

# Build targets
TARGET := x86_64-unknown-none
BIN_RELEASE := target/$(TARGET)/release/ziqa-kernel
BOOT_IMAGE_RELEASE := target/$(TARGET)/release/bootimage-ziqa-kernel.bin
BIN := target/$(TARGET)/debug/ziqa-kernel
BOOT_IMAGE := target/$(TARGET)/debug/bootimage-ziqa-kernel.bin

# Default build
default: build

# Fast debug build
build:
	cargo build --bin ziqa-kernel

# Release build
release:
	cargo build --release --bin ziqa-kernel

# Kill any leftover QEMU from interrupted runs (avoids "Failed to get write lock")
kill-qemu:
	-pkill -9 qemu-system-x86_64 2>/dev/null; sleep 0.5; true

# Run on QEMU (headless)
run: kill-qemu boot disk.img
	qemu-system-x86_64 -drive format=raw,file=$(BOOT_IMAGE) -drive file=disk.img,if=none,format=raw,id=hdr0 -device virtio-blk-pci,drive=hdr0 -m 512M -serial stdio -display none -device virtio-net-pci,netdev=net0 -netdev user,id=net0

# Run with graphical display AND interactive serial in THIS terminal.
# Uses TCP serial + socat to avoid Wayland/GTK keyboard-grab issues.
run-gui: kill-qemu
	./tools/run-gui.sh

# Run headless with terminal-only I/O (debug build).
run-headless: kill-qemu boot-gui disk.img
	qemu-system-x86_64 -drive format=raw,file=$(BOOT_IMAGE_RELEASE) -drive file=disk.img,if=none,format=raw,id=hdr0 -device virtio-blk-pci,drive=hdr0 -m 512M -serial stdio -display none -device virtio-net-pci,netdev=net0 -netdev user,id=net0

# Run with terminal I/O AND graphical display via VNC (connect with 'vncviewer :0')
run-vnc: kill-qemu boot-gui disk.img
	qemu-system-x86_64 -drive format=raw,file=$(BOOT_IMAGE_RELEASE) -drive file=disk.img,if=none,format=raw,id=hdr0 -device virtio-blk-pci,drive=hdr0 -m 512M -monitor none -serial stdio -vnc :0 -device virtio-net-pci,netdev=net0 -netdev user,id=net0

# Build the FAT32 disk image with orbital.elf.lz4 and development files.
# Uses mtools with offset syntax; no sudo needed.
disk.img: fat-disk
fat-disk:
	rm -f disk.img
	truncate -s 64M disk.img
	parted -s disk.img mklabel msdos mkpart primary fat32 1MiB 100% set 1 lba on
	mkfs.vfat --offset=2048 -F 32 disk.img
	mkdir -p fat-root/bin
	/tmp/decomp_tool/target/release/decomp_tool 2>/dev/null || true
	cp assets/orbital.elf fat-root/bin/ 2>/dev/null || true
	rm -f assets/orbital.elf 2>/dev/null || true
	echo 'Hello from host FAT32 disk' > fat-root/README.TXT
	mcopy -i disk.img@@1M -s fat-root/* ::/ 2>/dev/null || true
	rm -rf fat-root

# Clean build artifacts
clean:
	rm -rf target

# Test
test:
	cargo test

# Profile build (with sccache if available)
profile:
	rustc --version && cargo build --release

# Incremental rebuild (fast after changes)
inc:
	cargo build --bin ziqa-kernel --incremental

# Boot image
boot-gui:
	cargo build --release --target x86_64-unknown-none --bin ziqa-kernel --no-default-features --features "skip-self-tests orbital embed-orbital"
	CARGO_MANIFEST_DIR="$(CURDIR)" cargo bootimage --release --target x86_64-unknown-none --bin ziqa-kernel --no-default-features --features "skip-self-tests orbital embed-orbital"

boot: build
	CARGO_MANIFEST_DIR="$(CURDIR)" cargo bootimage

# Install sccache for faster rebuilds
install-sccache:
	cargo install sccache
	export PATH="$$HOME/.cargo/bin:$$PATH"

# Parallel build with max jobs
fast: 
	cargo build --bin ziqa-kernel -j $$(nproc)

# Update dependencies
update:
	cargo update

# Check Zig code compiles independently
zig-check:
	zig build-obj src/zig/blitter.zig -O ReleaseFast -target x86_64-freestanding-none -fPIC -fno-stack-protector -femit-bin=/dev/null 2>&1 || zig build-obj src/zig/blitter.zig -O ReleaseFast -target x86_64-freestanding-none -fPIC -fno-stack-protector