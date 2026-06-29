#!/usr/bin/env bash
# run-gui.sh — Build ZiqaKernel + Orbital and launch QEMU with GUI
set -euo pipefail

cd "$(dirname "$0")/.."

BOOTIMAGE="target/x86_64-unknown-none/release/bootimage-ziqa-kernel.bin"
DISK="disk.img"
ORBITAL_FEATURES="orbital skip-self-tests embed-orbital redox-debug"
ORBITAL_BIN="gui/orbital-master/target/x86_64-unknown-redox/release/orbital"
OLD_STTY=""
QEMU_DONE=0
QEMU_PID=""
SERIAL_PORT="${ZIQA_SERIAL_PORT:-4545}"
QEMU_DISPLAY="${ZIQA_QEMU_DISPLAY:-gtk,gl=off}"

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
 # Build Orbital GUI FIRST so include_bytes! picks up the fresh binary
 echo "Building Orbital GUI..."
 if [ -f ".cargo/config.toml" ]; then
     mv .cargo/config.toml .cargo/config.toml.temp
 fi
 cd gui/orbital-master
 redoxer build --release
 cd - > /dev/null
 echo "Compressing Orbital GUI..."
 cargo run --release --example compress_orbital
 if [ -f ".cargo/config.toml.temp" ]; then
     mv .cargo/config.toml.temp .cargo/config.toml
 fi
 
 # Build kernel with Orbital embedded so the GUI is available even when
 # the FAT disk is stale or the host-side Orbital rebuild fails.
 echo "Building kernel..."
 cargo build --release --bin ziqa-kernel --features "$ORBITAL_FEATURES"
 
 # Build bootimage with the same feature set.
 echo "Building bootimage..."
 cargo bootimage --release --bin ziqa-kernel --features "$ORBITAL_FEATURES"


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
mcopy -i "$DISK"@@1M -o fat-root/README.TXT ::/README.TXT < /dev/null 2>/dev/null || true
rm -rf fat-root
echo "FAT32 disk payload refreshed."

if ! command -v socat >/dev/null 2>&1; then
    echo "Error: socat is required for GUI serial attach." >&2
    echo "Install socat, or use 'make run-vnc'/'make run-headless'." >&2
    exit 1
fi

echo ""
echo "═══ Launching QEMU  ═══"
echo "  • Display: $QEMU_DISPLAY (override with ZIQA_QEMU_DISPLAY)"
echo "  • Serial: TCP 127.0.0.1:$SERIAL_PORT attached to this terminal"
echo "  • Exit serial: Ctrl-]"
echo ""

# Run QEMU in the background and attach serial through TCP. Keeping QEMU off
# stdio prevents GTK/Wayland keyboard grabs and raw terminal state from making
# the runner look frozen after the boot image is built.
qemu-system-x86_64 \
    -drive format=raw,file="$BOOTIMAGE" \
    -drive file="$DISK",if=none,format=raw,id=hdr0 \
    -device virtio-blk-pci,drive=hdr0 \
    -m 512M \
    -monitor none \
    -serial "tcp:127.0.0.1:${SERIAL_PORT},server=on,wait=on" \
    -device virtio-gpu-pci \
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