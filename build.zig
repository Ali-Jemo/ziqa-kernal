const std = @import("std");

pub fn build(b: *std.Build) void {
    const target = b.standardTargetOptions(.{});
    const optimize = b.standardOptimizeOption(.{
        .preferred_optimize_mode = .ReleaseFast,
    });

    // ── Zig Static Libraries (kernel hot-paths) ──────────────────────────

    const libblitter = b.addLibrary(.{
        .name = "blitter",
        .linkage = .static,
        .root_module = b.createModule(.{
            .root_source_file = b.path("src/zig/blitter.zig"),
            .target = target,
            .optimize = optimize,
        }),
    });
    libblitter.root_module.pic = true;
    b.installArtifact(libblitter);

    const libkernelops = b.addLibrary(.{
        .name = "kernelops",
        .linkage = .static,
        .root_module = b.createModule(.{
            .root_source_file = b.path("src/zig/kernel_ops.zig"),
            .target = target,
            .optimize = optimize,
        }),
    });
    libkernelops.root_module.pic = true;
    b.installArtifact(libkernelops);

    const libdemoclient = b.addLibrary(.{
        .name = "democlient",
        .linkage = .static,
        .root_module = b.createModule(.{
            .root_source_file = b.path("src/zig/demo_client.zig"),
            .target = target,
            .optimize = optimize,
        }),
    });
    libdemoclient.root_module.pic = true;
    b.installArtifact(libdemoclient);

    // ── doom.elf (standalone test binary) ─────────────────────────────────
    // When linking with doomgeneric C code, compile doom_port.zig as an
    // object and link it with C object files via `b.addObject`.

    const doom_elf = b.addExecutable(.{
        .name = "doom",
        .root_module = b.createModule(.{
            .root_source_file = b.path("src/zig/doom_port.zig"),
            .target = b.resolveTargetQuery(.{
                .cpu_arch = .x86_64,
                .os_tag = .linux,
                .abi = .musl,
            }),
            .optimize = optimize,
        }),
    });
    b.installArtifact(doom_elf);

    // ── Rust Kernel: cargo build (debug by default) ─────────────────────
    // Use -Dskip-cargo=true to skip cargo steps (build.rs passes this)

    const skip_cargo = b.option(bool, "skip-cargo", "Skip cargo build steps") orelse false;

    if (!skip_cargo) {
        const cargo_build = b.step("cargo-build", "Build the Rust kernel with cargo");
        const cargo_build_cmd = b.addSystemCommand(&.{
            "cargo", "build", "--bin", "ziqa-kernel",
        });
        cargo_build.dependOn(b.getInstallStep());
        cargo_build_cmd.step.dependOn(cargo_build);

        // ── Bootimage: cargo bootimage ──────────────────────────────────

        const bootimage_cmd = b.addSystemCommand(&.{
            "cargo", "bootimage",
        });
        bootimage_cmd.step.dependOn(&cargo_build_cmd.step);
        const bootimage = b.step("bootimage", "Build the bootable kernel image");
        bootimage.dependOn(&bootimage_cmd.step);

        // ── QEMU Runner ─────────────────────────────────────────────────

        const boot_img_path = "target/x86_64-unknown-none/debug/bootimage-ziqa-kernel.bin";
        const disk_img = "disk.img";

        const qemu_cmd = b.addSystemCommand(&.{
            "qemu-system-x86_64",
            "-drive", b.fmt("format=raw,file={s}", .{boot_img_path}),
            "-drive", b.fmt("file={s},if=none,format=raw,id=hdr0", .{disk_img}),
            "-device", "virtio-blk-pci,drive=hdr0",
            "-m", "512M",
            "-serial", "stdio",
            "-display", "none",
            "-device", "virtio-net-pci,netdev=net0",
            "-netdev", "user,id=net0",
        });
        qemu_cmd.step.dependOn(&bootimage_cmd.step);
        const run_qemu = b.step("run", "Boot the kernel in QEMU (serial only)");
        run_qemu.dependOn(&qemu_cmd.step);

        const qemu_gui_cmd = b.addSystemCommand(&.{
            "qemu-system-x86_64",
            "-drive", b.fmt("format=raw,file={s}", .{boot_img_path}),
            "-drive", b.fmt("file={s},if=none,format=raw,id=hdr0", .{disk_img}),
            "-m", "512M",
            "-serial", "stdio",
            "-display", "gtk",
            "-device", "virtio-net-pci,netdev=net0",
            "-netdev", "user,id=net0",
        });
        qemu_gui_cmd.step.dependOn(&bootimage_cmd.step);
        const run_gui = b.step("run-gui", "Boot the kernel in QEMU with graphical display");
        run_gui.dependOn(&qemu_gui_cmd.step);

        // ── Convenience: `zig build all` does everything ────────────────

        const all = b.step("all", "Build Zig libs + Rust kernel + bootimage");
        all.dependOn(&bootimage_cmd.step);
    }
}
