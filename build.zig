const std = @import("std");

pub fn build(b: *std.Build) void {
    const target = b.standardTargetOptions(.{});

    const optimize = b.standardOptimizeOption(.{
        .preferred_optimize_mode = .ReleaseFast,
    });

    // Blitter Static Library
    const libblitter = b.addLibrary(.{
        .name = "blitter",
        .linkage = .static,
        .root_module = b.createModule(.{
            .root_source_file = b.path("src/zig/blitter.zig"),
            .target = target,
            .optimize = optimize,
        }),
    });
    
    // Explicitly set PIC on the root module if possible, 
    // or on the compile step if accessible.
    libblitter.root_module.pic = true;

    b.installArtifact(libblitter);
}
