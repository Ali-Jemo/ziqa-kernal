/// ELF64 Loader for ZiqaKernel
///
/// Public surface: `load_elf(binary, process) -> Result<(), AbiError>`
///
/// Internal structure:
///   - `ElfBytes`  — bounds-checked cursor over the raw binary slice
///   - `parse_header` / `parse_phdr` — pure parsers, no I/O or logging
///   - `load_elf`  — the only function that logs; calls parsers then maps segments
use crate::abi::AbiError;
use crate::memory::{MemoryRegion, VirtAddr};
use crate::process::vma::Vma;
use crate::process::Process;

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

// ── Cursor ────────────────────────────────────────────────────────────────────

/// Bounds-checked read cursor over a byte slice.
struct ElfBytes<'a> {
    data: &'a [u8],
}

impl<'a> ElfBytes<'a> {
    fn new(data: &'a [u8]) -> Self { Self { data } }

    fn u16_at(&self, o: usize) -> Result<u16, AbiError> {
        self.data.get(o..o + 2)
            .map(|b| u16::from_le_bytes([b[0], b[1]]))
            .ok_or(AbiError::ParseError)
    }
    fn u32_at(&self, o: usize) -> Result<u32, AbiError> {
        self.data.get(o..o + 4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .ok_or(AbiError::ParseError)
    }
    fn u64_at(&self, o: usize) -> Result<u64, AbiError> {
        self.data.get(o..o + 8)
            .map(|b| u64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]))
            .ok_or(AbiError::ParseError)
    }
    fn len(&self) -> usize { self.data.len() }
}

// ── On-disk types ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy)]
struct Elf64Header {
    e_type:      u16,
    e_entry:     u64,
    e_phoff:     u64,
    e_phentsize: u16,
    e_phnum:     u16,
}

#[derive(Debug, Clone, Copy)]
struct Elf64Phdr {
    p_type:   u32,
    p_flags:  u32,
    p_offset: u64,
    p_vaddr:  u64,
    p_filesz: u64,
    p_memsz:  u64,
}

// ── Pure parsers (no logging) ─────────────────────────────────────────────────

fn parse_header(b: &ElfBytes) -> Result<Elf64Header, AbiError> {
    if b.len() < 64 { return Err(AbiError::ParseError); }
    if b.data[0..4] != ELF_MAGIC { return Err(AbiError::UnknownFormat); }
    if b.data[4] != ELFCLASS64  { return Err(AbiError::Other("Not 64-bit ELF")); }
    if b.data[5] != ELFDATA2LSB { return Err(AbiError::Other("Not little-endian ELF")); }
    let e_type    = b.u16_at(16)?;
    let e_machine = b.u16_at(18)?;
    if e_type != ET_EXEC && e_type != ET_DYN { return Err(AbiError::Other("Not executable ELF")); }
    if e_machine != EM_X86_64               { return Err(AbiError::Other("Not x86_64 ELF")); }
    Ok(Elf64Header {
        e_type,
        e_entry:     b.u64_at(24)?,
        e_phoff:     b.u64_at(32)?,
        e_phentsize: b.u16_at(54)?,
        e_phnum:     b.u16_at(56)?,
    })
}

fn parse_phdr(b: &ElfBytes, offset: usize) -> Result<Elf64Phdr, AbiError> {
    if b.len() < offset + 56 { return Err(AbiError::ParseError); }
    Ok(Elf64Phdr {
        p_type:   b.u32_at(offset)?,
        p_flags:  b.u32_at(offset + 4)?,
        p_offset: b.u64_at(offset + 8)?,
        p_vaddr:  b.u64_at(offset + 16)?,
        p_filesz: b.u64_at(offset + 32)?,
        p_memsz:  b.u64_at(offset + 40)?,
    })
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Load an ELF64 binary into a process.
///
/// Logging is confined to this function; the parsers above are pure.
pub fn load_elf(binary: &[u8], process: &mut Process) -> Result<(), AbiError> {
    let b = ElfBytes::new(binary);
    let hdr = parse_header(&b)?;

    crate::println!(
        "[ELF] entry=0x{:x} phdrs={} type={}",
        hdr.e_entry, hdr.e_phnum,
        if hdr.e_type == ET_EXEC { "EXEC" } else { "DYN" }
    );

    process.entry_point = VirtAddr::new(hdr.e_entry);
    process.cpu_state.rip = hdr.e_entry;

    let mut load_count = 0u32;
    let mut max_vaddr: u64 = 0;
    let mut has_interp = false;

    for i in 0..hdr.e_phnum as usize {
        let ph_off = hdr.e_phoff as usize + i * hdr.e_phentsize as usize;
        let phdr = parse_phdr(&b, ph_off)?;

        match phdr.p_type {
            PT_LOAD => {
                let end_vaddr = phdr.p_vaddr + phdr.p_memsz;
                if end_vaddr > 0x0000_7FFF_FFFF_FFFF {
                    return Err(AbiError::Other("ELF segment overlaps kernel space"));
                }
                let flags = crate::memory::paging::MemoryRegionFlags {
                    readable:       (phdr.p_flags & PF_R) != 0,
                    writable:       (phdr.p_flags & PF_W) != 0,
                    executable:     (phdr.p_flags & PF_X) != 0,
                    user_accessible: true,
                    copy_on_write:  false,
                };
                let region = MemoryRegion {
                    start: VirtAddr::new(phdr.p_vaddr),
                    size: phdr.p_memsz as usize,
                    flags,
                    is_file_backed: false,
                    file_path: None,
                    file_offset: phdr.p_offset,
                    bco_hook: None,
                };
                process.add_region(Vma::from(region));

                if end_vaddr > max_vaddr { max_vaddr = end_vaddr; }
                load_count += 1;
            }
            PT_INTERP => { has_interp = true; }
            _ => {}
        }
    }

    if has_interp {
        crate::println!("[ELF] WARNING: dynamic linker required (not supported)");
    }

    process.brk = (max_vaddr + 0xFFF) & !0xFFF;

    crate::println!("[ELF] {} segments registered, brk=0x{:x} (demand paged)", load_count, process.brk);
    Ok(())
}
