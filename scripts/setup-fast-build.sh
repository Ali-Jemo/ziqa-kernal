#!/bin/bash
# Build optimization script for ZiqaKernel
# This script sets up the fastest possible build configuration

set -e

echo "🚀 Setting up ultra-fast build configuration for ZiqaKernel..."

# Check if mold is installed
if ! command -v mold &> /dev/null; then
    echo "⚠️  mold linker not found. Installing..."
    if command -v pacman &> /dev/null; then
        sudo pacman -S mold --noconfirm
    elif command -v apt &> /dev/null; then
        sudo apt install mold -y
    else
        echo "❌ Please install mold manually: https://github.com/rui314/mold"
        exit 1
    fi
fi

# Check if sccache is installed
if ! command -v sccache &> /dev/null; then
    echo "⚠️  sccache not found. Installing..."
    if command -v cargo &> /dev/null; then
        cargo install sccache
    else
        echo "❌ Please install Rust first"
        exit 1
    fi
fi

# Enable sccache in cargo config
sed -i 's/# rustc-wrapper = "sccache"/rustc-wrapper = "sccache"/' .cargo/config.toml

echo "✅ Build optimization complete!"
echo ""
echo "📋 Summary of optimizations:"
echo "  • mold linker: 2-5x faster linking"
echo "  • sccache: compiler caching for instant rebuilds"
echo "  • opt-level = 1 for dev: faster compilation"
echo "  • opt-level = 2 for dependencies: optimized deps"
echo ""
echo "🔥 Build commands:"
echo "  cargo build                    # Fast dev build"
echo "  cargo build --profile dev-fast # Ultra-fast iteration"
echo "  cargo build --release          # Optimized release"
