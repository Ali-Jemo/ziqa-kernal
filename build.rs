use std::process::Command;

fn main() {
    // Decompress assets/orbital.elf.lz4 to assets/orbital.elf
    let compressed = std::fs::read("assets/orbital.elf.lz4").expect("Missing assets/orbital.elf.lz4");
    if let Ok(decompressed) = lz4_flex::decompress_size_prepended(&compressed) {
        std::fs::write("assets/orbital.elf", decompressed).expect("Failed to write assets/orbital.elf");
    }
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

        // Re-run if any zig source changes
        println!("cargo:rerun-if-changed=src/zig/blitter.zig");
        println!("cargo:rerun-if-changed=src/zig/kernel_ops.zig");
    }

    // Always re-run on build config changes
    println!("cargo:rerun-if-changed=build.zig");
    println!("cargo:rerun-if-changed=build.rs");
}
