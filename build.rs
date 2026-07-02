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

        // Re-run if any zig source changes
        println!("cargo:rerun-if-changed=src/zig/blitter.zig");
        println!("cargo:rerun-if-changed=src/zig/kernel_ops.zig");
    }

    // Always re-run on build config changes
    println!("cargo:rerun-if-changed=build.zig");
    println!("cargo:rerun-if-changed=build.rs");

    // Patch launcher ELF if present (needed for orbital feature)
    if std::env::var("CARGO_FEATURE_ORBITAL").is_ok() {
        patch_launcher_elf();
    }
}

/// Patch the launcher ELF binary from its original load address (0x400000)
/// to 0x100000000 to avoid collision with kernel rodata in the shared page table.
fn patch_launcher_elf() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR")
        .expect("CARGO_MANIFEST_DIR not set");
    let out_dir = std::env::var("OUT_DIR")
        .expect("OUT_DIR not set");

    let launcher_path = std::path::Path::new(&manifest_dir)
        .join("assets")
        .join("launcher.elf");
    let out_path = std::path::Path::new(&out_dir)
        .join("launcher_patched.elf");

    println!("cargo:rerun-if-changed={}", launcher_path.display());

    let binary = match std::fs::read(&launcher_path) {
        Ok(b) => b,
        Err(e) => {
            // launcher.elf may not exist (clean checkout, no launcher feature)
            eprintln!("note: launcher.elf not found ({}) — skipping patch", e);
            return;
        }
    };

    if binary.len() < 64 || &binary[..4] != b"\x7fELF" {
        eprintln!("note: launcher.elf has unexpected format — skipping patch");
        return;
    }

    // Read ELF64 header fields
    let e_type = u16::from_le_bytes([binary[16], binary[17]]);
    let e_entry = u64::from_le_bytes(binary[24..32].try_into().unwrap());
    let e_phoff = u64::from_le_bytes(binary[32..40].try_into().unwrap());
    let _e_shoff = u64::from_le_bytes(binary[40..48].try_into().unwrap());
    let e_phentsize = u16::from_le_bytes([binary[54], binary[55]]);
    let e_phnum = u16::from_le_bytes([binary[56], binary[57]]);

    if e_type != 2 {
        // ET_EXEC = 2, ET_DYN = 3
        eprintln!("note: launcher.elf is type {}, expected 2 (ET_EXEC)", e_type);
        return;
    }

    // Find current base address from PT_LOAD segments
    let mut old_base = u64::MAX;
    for i in 0..e_phnum {
        let off = e_phoff as usize + i as usize * e_phentsize as usize;
        if off + 8 > binary.len() { break; }
        let p_type = u32::from_le_bytes(binary[off..off+4].try_into().unwrap());
        if p_type == 1 {
            let p_vaddr = u64::from_le_bytes(binary[off+16..off+24].try_into().unwrap());
            if p_vaddr > 0 && p_vaddr < old_base {
                old_base = p_vaddr & !0xfff;
            }
        }
    }

    if old_base == u64::MAX {
        eprintln!("note: no PT_LOAD segments found in launcher.elf");
        return;
    }

    let new_base: u64 = 0x100000000; // 4 GiB — safely above kernel rodata ~9 MiB
    let delta = new_base.wrapping_sub(old_base);

    if delta == 0 {
        eprintln!("note: launcher.elf already at desired base");
        return;
    }

    let mut patched = binary.clone();

    // Patch program headers
    for i in 0..e_phnum {
        let off = e_phoff as usize + i as usize * e_phentsize as usize;
        if off + 48 > patched.len() { break; }
        let p_type = u32::from_le_bytes(patched[off..off+4].try_into().unwrap());
        if p_type == 1 {
            let old_vaddr = u64::from_le_bytes(patched[off+16..off+24].try_into().unwrap());
            if old_vaddr > 0 {
                let new_vaddr = old_vaddr.wrapping_add(delta);
                patched[off+16..off+24].copy_from_slice(&new_vaddr.to_le_bytes());
            }
        }
    }

    // Patch entry point
    let new_entry = e_entry.wrapping_add(delta);
    patched[24..32].copy_from_slice(&new_entry.to_le_bytes());

    // Patch GOT entries
    // Parse ELF section headers to find .got
    let e_shoff = u64::from_le_bytes(binary[40..48].try_into().unwrap());
    let e_shentsize = u16::from_le_bytes([binary[58], binary[59]]);
    let e_shnum = u16::from_le_bytes([binary[60], binary[61]]);
    let e_shstrndx = u16::from_le_bytes([binary[62], binary[63]]);

    if e_shoff > 0 && e_shentsize >= 64 {
        // First, find section header string table
        let strtab_off = e_shoff as usize + e_shstrndx as usize * e_shentsize as usize;
        let shstrtab_off = u64::from_le_bytes(binary[strtab_off+24..strtab_off+32].try_into().unwrap());

        for i in 0..e_shnum {
            let sh_off = e_shoff as usize + i as usize * e_shentsize as usize;
            if sh_off + 8 > patched.len() { break; }
            let sh_name = u32::from_le_bytes(patched[sh_off..sh_off+4].try_into().unwrap());
            let sh_offset = u64::from_le_bytes(patched[sh_off+24..sh_off+32].try_into().unwrap());
            let sh_size = u64::from_le_bytes(patched[sh_off+32..sh_off+40].try_into().unwrap());

            // Read section name
            let name_start = shstrtab_off as usize + sh_name as usize;
            let name_end = patched[name_start..].iter().position(|&b| b == 0).unwrap_or(16);
            let name = std::str::from_utf8(&patched[name_start..name_start + name_end.min(16)])
                .unwrap_or("");

            if name == ".got" && sh_offset > 0 && sh_size >= 8 {
                let end = (sh_offset + sh_size) as usize;
                for j in (sh_offset as usize..end).step_by(8) {
                    if j + 8 > patched.len() { break; }
                    let val = u64::from_le_bytes(patched[j..j+8].try_into().unwrap());
                    if val > 0x400000 && val < 0x800000 {
                        let new_val = val.wrapping_add(delta);
                        patched[j..j+8].copy_from_slice(&new_val.to_le_bytes());
                    }
                }
            }
        }
    }

    match std::fs::write(&out_path, &patched) {
        Ok(_) => println!("cargo:info=patched launcher.elf → {} (delta={:#x})", out_path.display(), delta),
        Err(e) => panic!("failed to write patched launcher: {}", e),
    }
}
