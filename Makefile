# Makefile for ZiqaKernel - Fast build system

SHELL := /bin/zsh

.PHONY: build run clean test profile

# Build targets
TARGET := x86_64-unknown-none
BIN := target/$(TARGET)/debug/ziqa-kernel

# Default build
default: build

# Fast debug build
build:
	cargo build --bin ziqa-kernel

# Release build
release:
	cargo build --release --bin ziqa-kernel

# Run on QEMU
run: build
	qemu-system-x86_64 -drive format=raw,file=$(BIN) -m 256M -display sdl

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
	bootimage create ./bootimage.toml 2>/dev/null || true
	bootimage build ./bootimage.toml 2>/dev/null || true

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
