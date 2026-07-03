#!/usr/bin/env bash
# run-gui.sh — Build ZiqaKernel + Orbital and launch QEMU with GUI
set -euo pipefail

cd "$(dirname "$0")/.."

BOOTIMAGE="target/x86_64-unknown-none/release/bootimage-ziqa-kernel.bin"
DISK="disk.img"
KERNEL_FEATURES="orbital skip-self-tests embed-orbital redox-debug"
ORBITAL_FEATURES="ziqa-bga-direct"
ORBITAL_BIN="gui/orbital-master/target/x86_64-unknown-redox/release/orbital"
OLD_STTY=""
QEMU_DONE=0
QEMU_PID=""
SERIAL_PORT="${ZIQA_SERIAL_PORT:-4545}"
SERIAL_LOG="${ZIQA_SERIAL_LOG:-/tmp/ziqa-gui-serial.log}"
ATTACH_SERIAL="${ZIQA_ATTACH_SERIAL:-0}"
QEMU_DISPLAY="${ZIQA_QEMU_DISPLAY:-gtk,gl=off}"

ensure_rust_src_for_bootimage() {
    local toolchain rust_sysroot rust_library_dir host_triple rust_llvm_tools_dir
    toolchain="$(rustup show active-toolchain 2>/dev/null | tail -n 1)"
    toolchain="${toolchain%% *}"
    rust_sysroot="$(rustc +"$toolchain" --print sysroot)"
    rust_library_dir="$rust_sysroot/lib/rustlib/src/rust/library"

    if [ ! -f "$rust_library_dir/Cargo.lock" ]; then
        echo "rust-src Cargo.lock missing; repairing rust-src for bootimage..."
        rustup component remove rust-src --toolchain "$toolchain" >/dev/null 2>&1 || true
        rustup component add rust-src --toolchain "$toolchain"

        if [ -f "$rust_library_dir/Cargo.toml" ] && [ ! -f "$rust_library_dir/Cargo.lock" ]; then
            cargo +"$toolchain" generate-lockfile --manifest-path "$rust_library_dir/Cargo.toml"
        fi
    fi

    if [ ! -f "$rust_library_dir/Cargo.lock" ]; then
        echo "Error: rust-src for $toolchain is missing $rust_library_dir/Cargo.lock after reinstall." >&2
        echo "Do not fake this file. Pin rust-toolchain.toml to nightly-2026-06-20 and rerun rustup component add rust-src llvm-tools-preview for that toolchain." >&2
        exit 1
    fi

    host_triple="$(rustc +"$toolchain" -vV | sed -n 's/^host: //p')"
    rust_llvm_tools_dir="$rust_sysroot/lib/rustlib/$host_triple/bin"

    if [ ! -x "$rust_llvm_tools_dir/llvm-objdump" ] || [ ! -x "$rust_llvm_tools_dir/llvm-objcopy" ]; then
        echo "llvm-tools missing; repairing llvm-tools-preview for bootimage..."
        rustup component remove llvm-tools-preview --toolchain "$toolchain" >/dev/null 2>&1 || true
        rustup component add llvm-tools-preview --toolchain "$toolchain"
    fi

    if [ ! -x "$rust_llvm_tools_dir/llvm-objdump" ] || [ ! -x "$rust_llvm_tools_dir/llvm-objcopy" ]; then
        echo "Error: llvm-tools for $toolchain is missing llvm-objdump or llvm-objcopy after reinstall." >&2
        echo "Pin rust-toolchain.toml to nightly-2026-06-20 and rerun rustup component add rust-src llvm-tools-preview for that toolchain." >&2
        exit 1
    fi
}

cleanup() {
    if [ -n "$OLD_STTY" ]; then
        stty "$OLD_STTY" 2>/dev/null || true
        OLD_STTY=""
    fi
    if [ -n "${QEMU_PID:-}" ] && kill -0 "$QEMU_PID" 2>/dev/null; then
        kill "$QEMU_PID" 2>/dev/null || true
        wait "$QEMU_PID" 2>/dev/null || true
    fi
    rm -rf fat-root
    if [ "$QEMU_DONE" -eq 0 ]; then
        QEMU_DONE=1
        echo ""
        echo "QEMU stopped."
    fi
}
trap cleanup EXIT INT TERM

# Kill any leftover QEMU
pkill -9 qemu-system-x86_64 2>/dev/null || true
sleep 0.5

echo "═══ Building ZiqaKernel + Orbital  ═══"
ensure_rust_src_for_bootimage
 # Build Orbital GUI FIRST so include_bytes! picks up the fresh binary
 echo "Building Orbital GUI..."
 if [ -f ".cargo/config.toml" ]; then
     mv .cargo/config.toml .cargo/config.toml.temp
 fi
 cd gui/orbital-master
 redoxer build --release --features "$ORBITAL_FEATURES"
 cd - > /dev/null
 echo "Compressing Orbital GUI..."
 cargo run --release --example compress_orbital
 if [ -f ".cargo/config.toml.temp" ]; then
     mv .cargo/config.toml.temp .cargo/config.toml
 fi
 
 # Build kernel with Orbital embedded so the GUI is available even when
 # the FAT disk is stale or the host-side Orbital rebuild fails.
 echo "Building kernel..."
cargo build --release --bin ziqa-kernel --no-default-features --features "$KERNEL_FEATURES"
 
 # Build bootimage with the same feature set.
 echo "Building bootimage..."
CARGO_MANIFEST_DIR="$PWD" cargo bootimage --release --bin ziqa-kernel --no-default-features --features "$KERNEL_FEATURES"


# Create disk image if missing, then refresh the Orbital payload on every run.
echo "Refreshing FAT32 disk payload..."
if [ ! -f "$DISK" ]; then
    echo "Creating disk image..."
    truncate -s 64M "$DISK"
    parted -s "$DISK" mklabel msdos mkpart primary fat32 1MiB 100% set 1 lba on 2>/dev/null || true
    mkfs.vfat --offset=2048 -F 32 "$DISK" 2>/dev/null || true
fi

mkdir -p fat-root/bin
echo 'Orbital GUI ready' > fat-root/README.TXT
if ! mdir -i "$DISK"@@1M ::/bin >/dev/null 2>&1; then
    mmd -i "$DISK"@@1M ::/bin < /dev/null >/dev/null 2>&1 || true
fi
if [ -f "$ORBITAL_BIN" ]; then
    cp "$ORBITAL_BIN" fat-root/bin/orbital.elf
    mcopy -i "$DISK"@@1M -o fat-root/bin/orbital.elf ::/bin/orbital.elf < /dev/null 2>/dev/null || true
else
    echo "Note: built Orbital binary not found; embedded Orbital asset will be used"
fi
# Copy terminal binary if available (from userspace/terminal build)
TERMINAL_BIN="userspace/terminal/target/x86_64-unknown-redox/release/terminal"
if [ -f "$TERMINAL_BIN" ]; then
    cp "$TERMINAL_BIN" fat-root/bin/terminal
    mcopy -i "$DISK"@@1M -o fat-root/bin/terminal ::/bin/terminal < /dev/null 2>/dev/null || true
else
    echo "Note: terminal binary not found; namespace will show as /bin/terminal (unmounted)"
fi
mcopy -i "$DISK"@@1M -o fat-root/README.TXT ::/README.TXT < /dev/null 2>/dev/null || true
rm -rf fat-root
echo "FAT32 disk payload refreshed."

if [ "$ATTACH_SERIAL" = "1" ] && ! command -v socat >/dev/null 2>&1; then
    echo "Error: socat is required when ZIQA_ATTACH_SERIAL=1." >&2
    echo "Install socat, or run GUI mode without serial attach." >&2
    exit 1
fi

if [ "$ATTACH_SERIAL" = "1" ]; then
    SERIAL_ARG="tcp:127.0.0.1:${SERIAL_PORT},server=on"
    SERIAL_NOTE="TCP 127.0.0.1:${SERIAL_PORT} attached to this terminal"
else
    rm -f "$SERIAL_LOG"
    SERIAL_ARG="file:${SERIAL_LOG}"
    SERIAL_NOTE="log file ${SERIAL_LOG} (set ZIQA_ATTACH_SERIAL=1 for shell)"
fi

echo ""
echo "═══ Launching QEMU  ═══"
echo "  • Display: $QEMU_DISPLAY (override with ZIQA_QEMU_DISPLAY)"
echo "  • Serial: $SERIAL_NOTE"
if [ "$ATTACH_SERIAL" = "1" ]; then
    echo "  • Exit serial: Ctrl-]"
fi
echo ""

# Run QEMU in the background. By default GUI mode writes serial to a log file
# instead of attaching host stdin to the kernel shell; otherwise typing while
# testing the GUI sends commands to the kernel serial console.
qemu-system-x86_64 \
    -drive format=raw,file="$BOOTIMAGE" \
    -drive file="$DISK",if=none,format=raw,id=hdr0 \
    -device virtio-blk-pci,drive=hdr0 \
    -m 512M \
    -monitor none \
    -serial "$SERIAL_ARG" \
    -display "$QEMU_DISPLAY" \
    -device virtio-net-pci,netdev=net0 \
    -netdev user,id=net0 &
QEMU_PID=$!

sleep 0.5
if ! kill -0 "$QEMU_PID" 2>/dev/null; then
    set +e
    wait "$QEMU_PID"
    QEMU_STATUS=$?
    set -e
    exit "$QEMU_STATUS"
fi

if [ "$ATTACH_SERIAL" = "1" ]; then
    if [ -t 0 ]; then
        OLD_STTY="$(stty -g)"
        SOCAT_STDIN="-,raw,echo=0,escape=0x1d"
    else
        SOCAT_STDIN="-"
    fi

    set +e
    socat "$SOCAT_STDIN" "TCP:127.0.0.1:${SERIAL_PORT}"
    SOCAT_STATUS=$?
    set -e

    if ! kill -0 "$QEMU_PID" 2>/dev/null; then
        set +e
        wait "$QEMU_PID"
        QEMU_STATUS=$?
        set -e
        exit "$QEMU_STATUS"
    fi
    exit "$SOCAT_STATUS"
fi

set +e
wait "$QEMU_PID"
QEMU_STATUS=$?
set -e
exit "$QEMU_STATUS"