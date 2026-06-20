#!/usr/bin/env bash
# run-gui.sh — single command: QEMU GUI window + serial shell in your terminal
set -euo pipefail

cd "$(dirname "$0")/.."

BOOTIMAGE="target/x86_64-unknown-none/release/bootimage-ziqa-kernel.bin"
DISK="disk.img"
OLD_STTY=""
QEMU_DONE=0

cleanup() {
    if [ -n "$OLD_STTY" ]; then
        stty "$OLD_STTY" 2>/dev/null || true
        OLD_STTY=""
    fi
    if [ "$QEMU_DONE" -eq 0 ]; then
        QEMU_DONE=1
        echo ""
        echo "QEMU stopped."
    fi
}
trap cleanup EXIT INT TERM

pkill -9 qemu-system-x86_64 2>/dev/null || true
sleep 0.5

# Rebuild unconditionally so `make run-gui` always launches the current kernel.
make boot-gui

# Create disk image if missing
if [ ! -f "$DISK" ]; then
    make disk.img
fi

echo "═══ ZiqaKernel — QEMU + GUI + Serial  ═══"
echo "  • GUI display: GTK window"
echo "  • Serial shell: this terminal via raw QEMU stdio"
echo ""
echo "  Type commands below. Ctrl+C to quit."
echo ""

if [ -t 0 ]; then
    OLD_STTY="$(stty -g)"
    # QEMU's stdio chardev does not reliably put every terminal into raw mode
    # when a GTK window is also active.  Force byte-at-a-time input here so the
    # shell receives keystrokes after boot, not local line-echo from the host.
    stty -echo -icanon min 1 time 0 isig -ixon -icrnl
fi

# Keep QEMU in the foreground and attach COM1 directly to this terminal.
# The previous TCP-serial+socat bridge was easy to leave focused on the GUI
# while the visible shell lived elsewhere. stdio gives one obvious input path.
qemu-system-x86_64 \
    -drive format=raw,file="$BOOTIMAGE" \
    -drive file="$DISK",if=none,format=raw,id=hdr0 \
    -device virtio-blk-pci,drive=hdr0 \
    -device virtio-gpu-pci \
    -m 512M \
    -monitor none \
    -serial stdio \
    -display gtk \
    -device virtio-net-pci,netdev=net0 \
    -netdev user,id=net0
