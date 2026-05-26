/// ELF64 Loader for ZiqaKernel
///
/// Parses ELF64 binaries and maps their LOAD segments into a Process.
/// Handles static-linked ELF64 executables (the kind produced by `musl-gcc -static`).

use crate::abi::AbiError;
use crate::memory::{VirtAddr, MemoryRegion, MemoryRegionFlags};
use crate::process::Process;
use crate::println;

// ELF64 constants
const ELF_MAGIC: [u8; 4] = [0x7F, b'E', b'L', b'F'];
const ELFCLASS64: u8 = 2;
const ELFDATA2LSB: u8 = 1;
const ET_EXEC: u16 = 2;
const ET_DYN: u16 = 3;
const EM_X86_64: u16 = 62;
const PT_LOAD: u32 = 1;
const PT_INTERP: u32 = 3;
const PF_X: u32 = 1;
const PF_W: u32 = 2;
const PF_R: u32 = 4;

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
struct Elf64Header {
    e_ident: [u8; 16],
    e_type: u16,
    e_machine: u16,
    e_version: u32,
    e_entry: u64,
    e_phoff: u64,
    e_shoff: u64,
    e_flags: u32,
    e_ehsize: u16,
    e_phentsize: u16,
    e_phnum: u16,
    e_shentsize: u16,
    e_shnum: u16,
    e_shstrndx: u16,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct Elf64Phdr {
    p_type: u32,
    p_flags: u32,
    p_offset: u64,
    p_vaddr: u64,
    p_paddr: u64,
    p_filesz: u64,
    p_memsz: u64,
    p_align: u64,
}

fn read_u16(d: &[u8], o: usize) -> u16 { u16::from_le_bytes([d[o], d[o+1]]) }
fn read_u32(d: &[u8], o: usize) -> u32 { u32::from_le_bytes([d[o],d[o+1],d[o+2],d[o+3]]) }
fn read_u64(d: &[u8], o: usize) -> u64 {
    u64::from_le_bytes([d[o],d[o+1],d[o+2],d[o+3],d[o+4],d[o+5],d[o+6],d[o+7]])
}

fn parse_header(data: &[u8]) -> Result<Elf64Header, AbiError> {
    if data.len() < 64 { return Err(AbiError::ParseError); }
    if data[0..4] != ELF_MAGIC { return Err(AbiError::UnknownFormat); }
    if data[4] != ELFCLASS64 { return Err(AbiError::Other("Not 64-bit ELF")); }
    if data[5] != ELFDATA2LSB { return Err(AbiError::Other("Not little-endian ELF")); }
    let e_type = read_u16(data, 16);
    let e_machine = read_u16(data, 18);
    if e_type != ET_EXEC && e_type != ET_DYN { return Err(AbiError::Other("Not executable ELF")); }
    if e_machine != EM_X86_64 { return Err(AbiError::Other("Not x86_64 ELF")); }
    let mut e_ident = [0u8; 16];
    e_ident.copy_from_slice(&data[0..16]);
    Ok(Elf64Header {
        e_ident, e_type, e_machine,
        e_version: read_u32(data, 20),
        e_entry: read_u64(data, 24),
        e_phoff: read_u64(data, 32),
        e_shoff: read_u64(data, 40),
        e_flags: read_u32(data, 48),
        e_ehsize: read_u16(data, 52),
        e_phentsize: read_u16(data, 54),
        e_phnum: read_u16(data, 56),
        e_shentsize: read_u16(data, 58),
        e_shnum: read_u16(data, 60),
        e_shstrndx: read_u16(data, 62),
    })
}

fn parse_phdr(data: &[u8], offset: usize) -> Result<Elf64Phdr, AbiError> {
    if data.len() < offset + 56 { return Err(AbiError::ParseError); }
    Ok(Elf64Phdr {
        p_type: read_u32(data, offset),
        p_flags: read_u32(data, offset + 4),
        p_offset: read_u64(data, offset + 8),
        p_vaddr: read_u64(data, offset + 16),
        p_paddr: read_u64(data, offset + 24),
        p_filesz: read_u64(data, offset + 32),
        p_memsz: read_u64(data, offset + 40),
        p_align: read_u64(data, offset + 48),
    })
}

fn phdr_flags_to_memory_flags(p_flags: u32) -> MemoryRegionFlags {
    MemoryRegionFlags {
        readable: (p_flags & PF_R) != 0,
        writable: (p_flags & PF_W) != 0,
        executable: (p_flags & PF_X) != 0,
        user_accessible: true,
    }
}

/// Load an ELF64 binary into a process.
pub fn load_elf(binary: &[u8], process: &mut Process) -> Result<(), AbiError> {
    let header = parse_header(binary)?;

    // Copy fields to avoid unaligned reference errors in println! and elsewhere
    let entry = header.e_entry;
    let phnum = header.e_phnum;
    let etype = header.e_type;
    let phoff = header.e_phoff;
    let phentsize = header.e_phentsize;

    println!("[ELF] entry=0x{:x} phdrs={} type={}",
        entry, phnum,
        if etype == ET_EXEC { "EXEC" } else { "DYN" });

    process.entry_point = VirtAddress::new(entry);
    process.cpu_state.rip = entry;

    let mut load_count = 0u32;
    for i in 0..phnum as usize {
        let ph_off = phoff as usize + i * phentsize as usize;
        let phdr = parse_phdr(binary, ph_off)?;

        // Copy phdr fields to avoid unaligned reference errors
        let p_type = phdr.p_type;
        let p_flags = phdr.p_flags;
        let p_vaddr = phdr.p_vaddr;
        let p_memsz = phdr.p_memsz;
        let p_offset = phdr.p_offset;

        match p_type {
            PT_LOAD => {
                let flags = phdr_flags_to_memory_flags(p_flags);
                let region = MemoryRegion {
                    start: VirtAddr::new(p_vaddr),
                    size: p_memsz as usize,
                    flags,
                    is_file_backed: true,
                    file_offset: p_offset,
                };
                if !process.add_region(region) {
                    return Err(AbiError::Other("Too many memory regions"));
                }
                load_count += 1;
            }
            PT_INTERP => {
                println!("[ELF] WARNING: dynamic linker required (not supported)");
            }
            _ => {}
        }
    }

    println!("[ELF] loaded {} segments", load_count);
    Ok(())
}
