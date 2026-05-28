use std::env;
use std::process::Command;

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();

    // Build blitter.zig into an object file for x86_64-freestanding-none.
    // We use build-obj because build-lib can be finicky with freestanding targets.
    let obj_path = format!("{}/blitter.o", out_dir);

    let status = Command::new("zig")
        .args(&[
            "build-obj",
            "src/zig/blitter.zig",
            "-O", "ReleaseFast",
            "-target", "x86_64-freestanding-none",
            // Must be PIC — the kernel is linked as a PIE binary
            "-fPIC",
            // Disable stack protector (no __stack_chk_fail in freestanding)
            "-fno-stack-protector",
            // Output path
            &format!("-femit-bin={}", obj_path),
        ])
        .status()
        .expect("Failed to execute zig. Is zig installed and on PATH?");

    if !status.success() {
        panic!("Zig compilation of blitter.zig failed");
    }

    // Create a static library archive from the object file using `ar`
    let lib_path = format!("{}/libblitter.a", out_dir);
    let ar_status = Command::new("ar")
        .args(&["rcs", &lib_path, &obj_path])
        .status()
        .expect("Failed to execute ar. Is binutils installed?");

    if !ar_status.success() {
        panic!("Failed to create libblitter.a archive");
    }

    // Tell cargo to link the static library
    println!("cargo:rustc-link-search=native={}", out_dir);
    println!("cargo:rustc-link-lib=static=blitter");

    // Re-run if any zig source changes
    println!("cargo:rerun-if-changed=src/zig/blitter.zig");
    println!("cargo:rerun-if-changed=build.rs");
}
