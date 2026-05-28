# Makefile for ZiqaKernel - Fast build system

SHELL := /bin/zsh

.PHONY: build run clean test profile

# Build targets
TARGET := x86_64-unknown-none
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

# Run on QEMU
run: boot
	qemu-system-x86_64 -drive format=raw,file=$(BOOT_IMAGE) -m 512M -serial stdio -display none

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
boot: build
	cargo bootimage

# Install sccache for faster rebuilds
install-sccache:
	cargo install sccache
	export PATH="$HOME/.cargo/bin:$PATH"

# Parallel build with max jobs
fast: 
	cargo build --bin ziqa-kernel -j $(nproc)

# Update dependencies
update:
	cargo update

# Check Zig code compiles independently
zig-check:
	zig build-obj src/zig/blitter.zig -O ReleaseFast -target x86_64-freestanding-none -fPIC -fno-stack-protector -femit-bin=/dev/null 2>&1 || zig build-obj src/zig/blitter.zig -O ReleaseFast -target x86_64-freestanding-none -fPIC -fno-stack-protector

# Run on QEMU with graphical display (for DOOM fire visual)
run-gui: boot
	qemu-system-x86_64 -drive format=raw,file=$(BOOT_IMAGE) -m 512M -serial stdio -display gtk
