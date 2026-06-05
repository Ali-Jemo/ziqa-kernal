/// Process Snapshotting — Instant-On Persistence Backend
///
/// Serializes a full process state (registers, VMAs, mapped page contents,
/// FD table, metadata) into a compact binary blob that can be persisted to
/// FAT32 and restored later to resume execution instantly.
///
/// ## Binary Snapshot Format (v1)
///
/// ```text
/// [MAGIC: 4 bytes "ZSNP"]
/// [VERSION: u32]
/// [CPU_STATE: CpuState — 160 bytes, 20×u64 LE]
/// [METADATA: pid, abi, priority, brk, mmap_bump, entry, stack — 7×u64]
/// [CWD_LEN: u32] [CWD: cwd_len bytes]
/// [VMA_COUNT: u32]
///   for each VMA:
///     [start: u64] [end: u64] [flags: u8] [page_data_len: u32] [page_data...]
/// [FD_COUNT: u32]
///   for each open FD:
///     [fd_index: u32] [target_tag: u8] [target_data: u32] [flags: u32] [offset: u64]
///     [path_len: u8] [path: path_len bytes]
/// ```

use alloc::vec::Vec;
use crate::process::{Process, CpuState, AbiKind, FdTarget, FdTable, FileDesc};
use crate::fs::vfs::VFS;
use crate::memory::PAGE_SIZE;
use x86_64::{VirtAddr, structures::paging::{Page, PageTableFlags, Mapper, OffsetPageTable}};

/// Magic bytes at the start of every snapshot file
const SNAP_MAGIC: &[u8; 4] = b"ZSNP";
const SNAP_VERSION: u32 = 1;

pub trait Snapshotable {
    fn snapshot(&self) -> Vec<u8>;
    fn restore(&mut self, data: &[u8]) -> bool;
}

pub struct SnapshotManager;

impl SnapshotManager {
    /// Save a process snapshot to FAT32 storage
    pub fn save(proc: &Process) -> bool {
        let snapshot = proc.snapshot();
        
        if let Some(compressed) = crate::memory::compression::COMPRESSION_ENGINE.compress(&snapshot) {
            let path = alloc::format!("/fat/snapshots/{}.snap", proc.pid.0);
            VFS.write().mkdir("/fat/snapshots");
            let mut vfs = VFS.write();
            vfs.create(&path);
            match vfs.write_raw(&path, &compressed, 0) {
                Ok(bytes) => {
                    crate::println!("[Snapshot] Saved PID {} → {} ({} bytes, compressed from {})",
                        proc.pid.0, path, bytes, snapshot.len());
                    true
                }
                Err(e) => {
                    crate::println!("[Snapshot] Failed to persist PID {}: {:?}", proc.pid.0, e);
                    false
                }
            }
        } else {
            crate::println!("[Snapshot] Compression failed for PID {}", proc.pid.0);
            false
        }
    }

    /// Load and restore a process snapshot from storage
    pub fn load(pid: u64, proc: &mut Process) -> bool {
        let path = alloc::format!("/fat/snapshots/{}.snap", pid);

        let vfs = VFS.read();
        let compressed = match vfs.read_raw_all(&path) {
            Ok(data) => data,
            Err(e) => {
                crate::println!("[Snapshot] Failed to read {}: {:?}", path, e);
                return false;
            }
        };
        drop(vfs);

        // lz4_flex stores original_size in the prepended header, so pass 0
        if let Some(snapshot) = crate::memory::compression::COMPRESSION_ENGINE.decompress(&compressed, 0) {
            if proc.restore(&snapshot) {
                crate::println!("[Snapshot] Restored PID {} from {} ({} bytes)", pid, path, snapshot.len());
                true
            } else {
                crate::println!("[Snapshot] Failed to restore process state for PID {}", pid);
                false
            }
        } else {
            crate::println!("[Snapshot] Decompression failed for {}", path);
            false
        }
    }

    /// List all saved snapshots in /fat/snapshots/
    pub fn list() -> Vec<u64> {
        let mut pids = Vec::new();
        let vfs = VFS.read();
        let entries = vfs.list_dir("/fat/snapshots");
        for entry in entries {
            // list_dir returns full paths like "/fat/snapshots/42.snap"
            let filename = entry.rsplit('/').next().unwrap_or(&entry);
            if filename.ends_with(".snap") {
                let name = &filename[..filename.len() - 5];
                if let Ok(pid) = name.parse::<u64>() {
                    pids.push(pid);
                }
            }
        }
        pids
    }

    /// Delete a saved snapshot
    pub fn delete(pid: u64) -> bool {
        let path = alloc::format!("/fat/snapshots/{}.snap", pid);
        let mut vfs = VFS.write();
        match vfs.remove(&path) {
            Ok(()) => {
                crate::println!("[Snapshot] Deleted snapshot for PID {}", pid);
                true
            }
            Err(e) => {
                crate::println!("[Snapshot] Failed to delete snapshot for PID {}: {:?}", pid, e);
                false
            }
        }
    }
}

/// Restore all saved snapshots at boot time.
///
/// Called during startup after the filesystem and scheduler are ready.
/// Creates a placeholder process for each saved snapshot and restores
/// the full process state (registers, memory, FDs) from the snapshot file.
///
/// `placeholder_binary` is an ELF binary used to create the initial process
/// before `SnapshotManager::load()` overwrites its state.
pub fn restore_all_at_boot(placeholder_binary: &[u8]) -> usize {
    let pids = SnapshotManager::list();
    if pids.is_empty() {
        crate::println!("[Snapshot] No saved snapshots found");
        return 0;
    }

    let mut restored = 0usize;
    for snap_pid in &pids {
        crate::println!("[Snapshot] Restoring PID {} from snapshot...", snap_pid);

        // Spawn a placeholder process
        let new_pid = match crate::process::scheduler::spawn_elf(placeholder_binary) {
            Some(pid) => pid,
            None => {
                crate::println!("[Snapshot] Failed to spawn placeholder for PID {}", snap_pid);
                continue;
            }
        };

        // Restore the snapshot state into the placeholder
        let ok = crate::process::scheduler::with_process_mut(new_pid, |proc| {
            SnapshotManager::load(*snap_pid, proc)
        }).unwrap_or(false);

        if ok {
            crate::println!("[Snapshot] Restored PID {} (new PID={})", snap_pid, new_pid.0);
            restored += 1;
        } else {
            crate::println!("[Snapshot] Failed to restore PID {} into {}", snap_pid, new_pid.0);
            // Kill the failed placeholder
            crate::process::scheduler::SCHEDULER.send_signal(new_pid, 9);
        }
    }

    crate::println!("[Snapshot] Restored {}/{} processes from boot snapshots", restored, pids.len());
    restored
}

// ── Serialization helpers ─────────────────────────────────────────────────────

fn push_u32(buf: &mut Vec<u8>, v: u32) {
    buf.extend_from_slice(&v.to_le_bytes());
}

fn push_u64(buf: &mut Vec<u8>, v: u64) {
    buf.extend_from_slice(&v.to_le_bytes());
}

fn read_u32(data: &[u8], offset: &mut usize) -> Option<u32> {
    if *offset + 4 > data.len() { return None; }
    let v = u32::from_le_bytes([data[*offset], data[*offset+1], data[*offset+2], data[*offset+3]]);
    *offset += 4;
    Some(v)
}

fn read_u64(data: &[u8], offset: &mut usize) -> Option<u64> {
    if *offset + 8 > data.len() { return None; }
    let bytes: [u8; 8] = data[*offset..*offset+8].try_into().ok()?;
    let v = u64::from_le_bytes(bytes);
    *offset += 8;
    Some(v)
}

fn read_u8(data: &[u8], offset: &mut usize) -> Option<u8> {
    if *offset >= data.len() { return None; }
    let v = data[*offset];
    *offset += 1;
    Some(v)
}

fn read_bytes<'a>(data: &'a [u8], offset: &mut usize, len: usize) -> Option<&'a [u8]> {
    if *offset + len > data.len() { return None; }
    let slice = &data[*offset..*offset + len];
    *offset += len;
    Some(slice)
}

// ── Encode VMA flags as a single byte ─────────────────────────────────────────

fn encode_flags(flags: &crate::memory::paging::MemoryRegionFlags) -> u8 {
    let mut v = 0u8;
    if flags.readable       { v |= 0x01; }
    if flags.writable       { v |= 0x02; }
    if flags.executable     { v |= 0x04; }
    if flags.user_accessible { v |= 0x08; }
    if flags.copy_on_write  { v |= 0x10; }
    v
}

fn decode_flags(v: u8) -> crate::memory::paging::MemoryRegionFlags {
    crate::memory::paging::MemoryRegionFlags {
        readable:        v & 0x01 != 0,
        writable:        v & 0x02 != 0,
        executable:      v & 0x04 != 0,
        user_accessible: v & 0x08 != 0,
        copy_on_write:   v & 0x10 != 0,
    }
}

// ── FdTarget encoding ─────────────────────────────────────────────────────────

fn encode_fd_target(target: &FdTarget) -> (u8, u32) {
    match target {
        FdTarget::Stdin      => (0, 0),
        FdTarget::Stdout     => (1, 0),
        FdTarget::Stderr     => (2, 0),
        FdTarget::PipeRead(id)  => (3, *id),
        FdTarget::PipeWrite(id) => (4, *id),
        FdTarget::File(idx)     => (5, *idx as u32),
    }
}

fn decode_fd_target(tag: u8, data: u32) -> Option<FdTarget> {
    match tag {
        0 => Some(FdTarget::Stdin),
        1 => Some(FdTarget::Stdout),
        2 => Some(FdTarget::Stderr),
        3 => Some(FdTarget::PipeRead(data)),
        4 => Some(FdTarget::PipeWrite(data)),
        5 => Some(FdTarget::File(data as u8)),
        _ => None,
    }
}

// ── Snapshotable implementation ───────────────────────────────────────────────

impl Snapshotable for Process {
    fn snapshot(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(4096);
        
        // Header
        buf.extend_from_slice(SNAP_MAGIC);
        push_u32(&mut buf, SNAP_VERSION);
        
        // CPU state — all 20 u64 fields in struct order
        let cpu = &self.cpu_state;
        for &val in &[
            cpu.r15, cpu.r14, cpu.r13, cpu.r12, cpu.r11,
            cpu.r10, cpu.r9,  cpu.r8,  cpu.rdi, cpu.rsi,
            cpu.rbp, cpu.rbx, cpu.rdx, cpu.rcx, cpu.rax,
            cpu.rip, cpu.cs,  cpu.rflags, cpu.rsp, cpu.ss,
        ] {
            push_u64(&mut buf, val);
        }
        
        // Metadata
        push_u64(&mut buf, self.pid.0);
        push_u64(&mut buf, match self.abi {
            AbiKind::LinuxElf => 0,
            AbiKind::Wasm => 1,
            AbiKind::ZiqaNative => 2,
        });
        push_u64(&mut buf, self.priority as u64);
        push_u64(&mut buf, self.brk);
        push_u64(&mut buf, self.mmap_bump);
        push_u64(&mut buf, self.entry_point.as_u64());
        push_u64(&mut buf, self.stack_top.as_u64());
        
        // CWD
        push_u32(&mut buf, self.cwd_len as u32);
        buf.extend_from_slice(&self.cwd[..self.cwd_len]);
        
        // VMAs — for each, snapshot the actual page contents
        push_u32(&mut buf, self.vmas.len() as u32);
        for vma in &self.vmas {
            push_u64(&mut buf, vma.start.as_u64());
            push_u64(&mut buf, vma.end.as_u64());
            buf.push(encode_flags(&vma.flags));
            
            // Read the actual mapped memory pages for this VMA.
            let vma_size = (vma.end.as_u64() - vma.start.as_u64()) as usize;
            let page_count = (vma_size + PAGE_SIZE - 1) / PAGE_SIZE;
            
            // Collect page data: for each page, store a presence byte + contents
            let mut page_data = Vec::new();
            for page_idx in 0..page_count {
                let page_addr = VirtAddr::new(vma.start.as_u64() + (page_idx * PAGE_SIZE) as u64);
                
                // Check if this page is actually mapped in the process's own page table
                let entry = if let Some(frame) = self.page_table_frame {
                    crate::memory::paging::get_leaf_entry_mut_in(frame, page_addr)
                } else {
                    crate::memory::paging::get_leaf_entry_mut(page_addr)
                };

                if let Some(entry) = entry {
                    if entry.flags().contains(PageTableFlags::PRESENT) {
                        let phys_addr = entry.addr();
                        let virt = crate::memory::paging::phys_offset() + phys_addr.as_u64();
                        let src = virt.as_ptr::<u8>();
                        
                        page_data.push(1u8); // present
                        unsafe {
                            let page_bytes = core::slice::from_raw_parts(src, PAGE_SIZE);
                            page_data.extend_from_slice(page_bytes);
                        }
                        continue;
                    }
                }
                page_data.push(0u8); // not present / not mapped
            }
            
            push_u32(&mut buf, page_data.len() as u32);
            buf.extend_from_slice(&page_data);
        }
        
        // FD table — only open fds
        let mut fd_count = 0u32;
        let fd_count_pos = buf.len();
        push_u32(&mut buf, 0); // placeholder
        
        for fd_idx in 0..8 {
            if let Some(desc) = self.fds.get(fd_idx) {
                fd_count += 1;
                push_u32(&mut buf, fd_idx as u32);
                let (tag, target_data) = encode_fd_target(&desc.target);
                buf.push(tag);
                push_u32(&mut buf, target_data);
                push_u32(&mut buf, desc.flags);
                push_u64(&mut buf, desc.offset as u64);
                
                // Path data
                let path_len = self.fds.path_lens[fd_idx];
                buf.push(path_len as u8);
                if path_len > 0 {
                    buf.extend_from_slice(&self.fds.paths[fd_idx][..path_len]);
                }
            }
        }
        
        // Patch fd_count
        let count_bytes = fd_count.to_le_bytes();
        buf[fd_count_pos..fd_count_pos+4].copy_from_slice(&count_bytes);
        
        buf
    }
    
    fn restore(&mut self, data: &[u8]) -> bool {
        self.restore_inner(data).is_some()
    }
}

/// Inner restore that returns Option so we can use the ? operator freely.
impl Process {
    fn restore_inner(&mut self, data: &[u8]) -> Option<()> {
        let mut off = 0usize;
        
        // Verify magic
        let magic = read_bytes(data, &mut off, 4)?;
        if magic != SNAP_MAGIC {
            crate::println!("[Snapshot] Bad magic: {:?}", magic);
            return None;
        }
        
        // Version
        let version = read_u32(data, &mut off)?;
        if version != SNAP_VERSION {
            crate::println!("[Snapshot] Unsupported snapshot version: {}", version);
            return None;
        }
        
        // CPU state — read all 20 u64 fields
        let mut regs = [0u64; 20];
        for reg in regs.iter_mut() {
            *reg = read_u64(data, &mut off)?;
        }
        self.cpu_state = CpuState {
            r15: regs[0],  r14: regs[1],  r13: regs[2],  r12: regs[3],
            r11: regs[4],  r10: regs[5],  r9:  regs[6],  r8:  regs[7],
            rdi: regs[8],  rsi: regs[9],  rbp: regs[10], rbx: regs[11],
            rdx: regs[12], rcx: regs[13], rax: regs[14], rip: regs[15],
            cs:  regs[16], rflags: regs[17], rsp: regs[18], ss: regs[19],
        };
        
        // Metadata
        let snap_pid = read_u64(data, &mut off)?;
        let abi_val = read_u64(data, &mut off)?;
        self.abi = match abi_val {
            0 => AbiKind::LinuxElf,
            1 => AbiKind::Wasm,
            2 => AbiKind::ZiqaNative,
            _ => return None,
        };
        self.priority = read_u64(data, &mut off)? as u8;
        self.brk = read_u64(data, &mut off)?;
        self.mmap_bump = read_u64(data, &mut off)?;
        self.entry_point = x86_64::VirtAddr::new(read_u64(data, &mut off)?);
        self.stack_top = x86_64::VirtAddr::new(read_u64(data, &mut off)?);
        
        // CWD
        let cwd_len = read_u32(data, &mut off)? as usize;
        if cwd_len > 128 { return None; }
        let cwd_bytes = read_bytes(data, &mut off, cwd_len)?;
        self.cwd[..cwd_len].copy_from_slice(cwd_bytes);
        self.cwd_len = cwd_len;
        
            // VMAs — restore virtual memory mappings and page contents
            let vma_count = read_u32(data, &mut off)? as usize;
            self.vmas.clear();
            
            // Ensure we have a page table frame
            if self.page_table_frame.is_none() {
                self.page_table_frame = crate::memory::paging::create_process_page_table();
            }
            let root_frame = self.page_table_frame?;
            
            // Build a mapper from the process's root page table
            let po = crate::memory::paging::phys_offset();
            let l4_virt = po + root_frame.start_address().as_u64();
            let l4 = unsafe { &mut *(l4_virt.as_mut_ptr()) };
            let mut mapper = unsafe { OffsetPageTable::new(l4, po) };

            for _ in 0..vma_count {
                let start = read_u64(data, &mut off)?;
                let end = read_u64(data, &mut off)?;
                let flags_byte = read_u8(data, &mut off)?;
                let flags = decode_flags(flags_byte);
                
                let page_data_len = read_u32(data, &mut off)? as usize;
                let page_data = read_bytes(data, &mut off, page_data_len)?;
                
                let vma = crate::process::vma::Vma::new(
                    VirtAddr::new(start),
                    (end - start) as usize,
                    flags,
                );
                self.vmas.push(vma);
                
                // Restore page contents: walk the page data and map each present page
                let page_count = ((end - start) as usize + PAGE_SIZE - 1) / PAGE_SIZE;
                let mut pd_off = 0usize;
                
                for page_idx in 0..page_count {
                    if pd_off >= page_data.len() { break; }
                    
                    let present = page_data[pd_off];
                    pd_off += 1;
                    
                    if present == 1 {
                        if pd_off + PAGE_SIZE > page_data.len() {
                            crate::println!("[Snapshot] Truncated page data at VMA 0x{:x}", start);
                            return None;
                        }
                        let page_contents = &page_data[pd_off..pd_off + PAGE_SIZE];
                        pd_off += PAGE_SIZE;
                        
                        let page_vaddr = VirtAddr::new(start + (page_idx * PAGE_SIZE) as u64);
                        
                        // Allocate a physical frame
                        let mut fa_guard = crate::memory::FRAME_ALLOCATOR.lock();
                        let fa = fa_guard.as_mut().expect("FRAME_ALLOCATOR not initialized");
                        use x86_64::structures::paging::FrameAllocator;
                        let frame = match fa.allocate_frame() {
                            Some(f) => f,
                            None => {
                                crate::println!("[Snapshot] OOM restoring page at {:?}", page_vaddr);
                                return None;
                            }
                        };
                        drop(fa_guard);
                        
                        // Copy page data into the frame via phys offset
                        let phys_addr = frame.start_address();
                        let dst_virt = crate::memory::paging::phys_offset() + phys_addr.as_u64();
                        unsafe {
                            core::ptr::copy_nonoverlapping(
                                page_contents.as_ptr(),
                                dst_virt.as_mut_ptr::<u8>(),
                                PAGE_SIZE,
                            );
                        }
                        
                        // Build PTE flags from VMA flags
                        let mut ptflags = PageTableFlags::PRESENT;
                        if flags.writable        { ptflags |= PageTableFlags::WRITABLE; }
                        if flags.user_accessible { ptflags |= PageTableFlags::USER_ACCESSIBLE; }
                        if !flags.executable     { ptflags |= PageTableFlags::NO_EXECUTE; }
                        
                        let page = Page::<x86_64::structures::paging::Size4KiB>::containing_address(page_vaddr);
                        
                        // Map via the process-specific mapper we built
                        let mut fa_guard2 = crate::memory::FRAME_ALLOCATOR.lock();
                        let fa2 = fa_guard2.as_mut().expect("FRAME_ALLOCATOR not initialized");
                        unsafe {
                            // Ignore AlreadyMapped — page may already exist from a prior snapshot
                            let _ = mapper.map_to(page, frame, ptflags, fa2);
                        }
                    }
                }
            }
        
        // FD table
        let fd_count = read_u32(data, &mut off)? as usize;
        self.fds = FdTable::new();
        
        for _ in 0..fd_count {
            let fd_idx = read_u32(data, &mut off)? as usize;
            let tag = read_u8(data, &mut off)?;
            let target_data = read_u32(data, &mut off)?;
            let flags = read_u32(data, &mut off)?;
            let offset = read_u64(data, &mut off)? as usize;
            let path_len = read_u8(data, &mut off)? as usize;
            let path_bytes = if path_len > 0 {
                read_bytes(data, &mut off, path_len)?
            } else {
                &[][..]
            };
            
            if let Some(target) = decode_fd_target(tag, target_data) {
                if fd_idx < 8 {
                    if fd_idx < 3 {
                        // Update stdio fd metadata
                        if let Some(desc) = self.fds.get_mut(fd_idx) {
                            desc.flags = flags;
                            desc.offset = offset;
                        }
                    } else {
                        // Restore file/pipe fd at exact index
                        let desc = FileDesc { target, flags, offset };
                        let n = path_len.min(63);
                        if n > 0 {
                            self.fds.paths[fd_idx][..n].copy_from_slice(&path_bytes[..n]);
                        }
                        self.fds.path_lens[fd_idx] = n;
                        if let Some(slot) = self.fds.get_mut(fd_idx) {
                            *slot = desc;
                        }
                    }
                }
            }
        }
        
        if snap_pid != self.pid.0 {
            crate::println!("[Snapshot] Note: original PID {} → current PID {}", snap_pid, self.pid.0);
        }
        
        Some(())
    }
}
