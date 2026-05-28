use std::process::Command;

fn main() {
    // Invoke `zig build` to build the blitter library
    let status = Command::new("zig")
        .args(&[
            "build",
            "-Dtarget=x86_64-freestanding-none",
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

    // Re-run if any zig source changes
    println!("cargo:rerun-if-changed=src/zig/blitter.zig");
    println!("cargo:rerun-if-changed=build.zig");
    println!("cargo:rerun-if-changed=build.rs");
}
