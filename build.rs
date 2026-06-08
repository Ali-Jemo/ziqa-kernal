use std::process::Command;

fn main() {
    // Check if zig-hotpaths feature is enabled
    let link_zig = std::env::var("CARGO_FEATURE_ZIG_HOTPATHS").is_ok();

    if link_zig {
        // Invoke `zig build` to build the blitter library
        let status = Command::new("zig")
            .args(&[
                "build",
                "-Dtarget=x86_64-freestanding-none",
                "-Dskip-cargo=true",
                "--release=fast",
            ])
            .status()
            .expect("Failed to execute zig build. Is zig installed?");

        if !status.success() {
            panic!("Zig build of blitter library failed");
        }

        // Tell cargo where to find the static library
        println!("cargo:rustc-link-search=native=zig-out/lib");
        println!("cargo:rustc-link-lib=static=blitter");
        println!("cargo:rustc-link-lib=static=kernelops");
        println!("cargo:rustc-link-lib=static=democlient");

        // Re-run if any zig source changes
        println!("cargo:rerun-if-changed=src/zig/blitter.zig");
        println!("cargo:rerun-if-changed=src/zig/kernel_ops.zig");
        println!("cargo:rerun-if-changed=src/zig/demo_client.zig");
    }

    // Always re-run on build config changes
    println!("cargo:rerun-if-changed=build.zig");
    println!("cargo:rerun-if-changed=build.rs");
}
