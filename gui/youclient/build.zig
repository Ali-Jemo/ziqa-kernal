const std = @import("std");

pub fn build(b: *std.Build) void {
    const optimize = b.standardOptimizeOption(.{
        .preferred_optimize_mode = .ReleaseFast,
    });

    const youcanvas_mod = b.createModule(.{
        .root_source_file = b.path("../youcanvas/src/root.zig"),
    });

    const youui_mod = b.createModule(.{
        .root_source_file = b.path("../youui/src/root.zig"),
    });
    youui_mod.addImport("youcanvas", youcanvas_mod);

    const exe = b.addExecutable(.{
        .name = "demo_client.elf",
        .root_module = b.createModule(.{
            .root_source_file = b.path("src/demo_client.zig"),
            .target = b.resolveTargetQuery(.{
                .cpu_arch = .x86_64,
                .os_tag = .freestanding,
                .abi = .none,
            }),
            .optimize = optimize,
        }),
    });
    
    exe.root_module.addImport("youcanvas", youcanvas_mod);
    exe.root_module.addImport("youui", youui_mod);
    
    exe.root_module.pic = true;
    exe.root_module.omit_frame_pointer = true;
    
    b.installArtifact(exe);
}