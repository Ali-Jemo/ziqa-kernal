/// FAT32 Filesystem Driver for ZiqaKernel
///
/// Implements manual FAT32 parsing (no external crate dependency) including:
/// - MBR partition table scanning for FAT32 partitions
/// - BPB (BIOS Parameter Block) / boot sector parsing
/// - FAT cluster chain traversal
/// - Directory entry parsing (short names; LFN entries are skipped)
/// - Read-only file access via the kernel `File` trait
///
/// Each discovered file is mounted into the VFS as a `Fat32File` that
/// lazily reads cluster chains from the underlying block device.
extern crate alloc;

use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use spin::Mutex;
use lazy_static::lazy_static;

use crate::abi::AbiError;
use crate::drivers::block::{BlockDevice, SECTOR_SIZE};
use crate::fs::vfs::VFS;
use crate::fs::{File, FileType};

// ─── Error Type ──────────────────────────────────────────────────────────────

/// Errors specific to FAT32 operations.
#[derive(Debug)]
pub enum Fat32Error {
    /// The boot sector signature (0xAA55) is missing.
    BadSignature,
    /// A field in the BPB has an unexpected / unsupported value.
    InvalidBpb(&'static str),
    /// Underlying block-device I/O failed.
    IoError,
    /// The partition table entry does not indicate FAT32.
    NotFat32,
}

// ─── Boot Sector / BPB ──────────────────────────────────────────────────────

/// Parsed FAT32 BIOS Parameter Block fields we actually need.
#[derive(Debug, Clone, Copy)]
pub struct Fat32Bpb {
    bytes_per_sector: u16,
    sectors_per_cluster: u8,
    reserved_sectors: u16,
    num_fats: u8,
    sectors_per_fat_32: u32,
    root_cluster: u32,
    /// Absolute sector on the disk where this partition starts.
    partition_start: u64,
}

impl Fat32Bpb {
    /// Parse BPB from a 512-byte boot sector buffer.
    fn parse(sector: &[u8], partition_start: u64) -> Result<Self, Fat32Error> {
        if sector.len() < 512 {
            return Err(Fat32Error::InvalidBpb("sector too short"));
        }
        // Signature check at offset 0x1FE
        let sig = u16::from_le_bytes([sector[0x1FE], sector[0x1FF]]);
        if sig != 0xAA55 {
            return Err(Fat32Error::BadSignature);
        }

        let bytes_per_sector = u16::from_le_bytes([sector[0x0B], sector[0x0C]]);
        if bytes_per_sector == 0 || (bytes_per_sector & (bytes_per_sector - 1)) != 0 {
            return Err(Fat32Error::InvalidBpb("bytes_per_sector not power-of-2"));
        }

        let sectors_per_cluster = sector[0x0D];
        if sectors_per_cluster == 0 {
            return Err(Fat32Error::InvalidBpb("sectors_per_cluster is zero"));
        }

        let reserved_sectors = u16::from_le_bytes([sector[0x0E], sector[0x0F]]);
        let num_fats = sector[0x10];
        if num_fats == 0 {
            return Err(Fat32Error::InvalidBpb("num_fats is zero"));
        }

        let sectors_per_fat_32 = u32::from_le_bytes([
            sector[0x24],
            sector[0x25],
            sector[0x26],
            sector[0x27],
        ]);

        let root_cluster = u32::from_le_bytes([
            sector[0x2C],
            sector[0x2D],
            sector[0x2E],
            sector[0x2F],
        ]);

        Ok(Self {
            bytes_per_sector,
            sectors_per_cluster,
            reserved_sectors,
            num_fats,
            sectors_per_fat_32,
            root_cluster,
            partition_start,
        })
    }

    /// First sector of the FAT region (absolute on disk).
    fn fat_start_sector(&self) -> u64 {
        self.partition_start + self.reserved_sectors as u64
    }

    /// First sector of the data region (absolute on disk).
    fn data_start_sector(&self) -> u64 {
        self.fat_start_sector() + (self.num_fats as u64) * (self.sectors_per_fat_32 as u64)
    }

    /// Convert a cluster number to an absolute sector on the disk.
    fn cluster_to_sector(&self, cluster: u32) -> u64 {
        self.data_start_sector() + ((cluster - 2) as u64) * (self.sectors_per_cluster as u64)
    }
}

// ─── FAT Chain Following ────────────────────────────────────────────────────

/// Read the next cluster number from the FAT for `current_cluster`.
///
/// Returns `Some(next)` if the chain continues, or `None` if this is the
/// last cluster (end-of-chain marker >= 0x0FFFFFF8) or an error occurs.
fn follow_cluster_chain(
    disk: &dyn BlockDevice,
    bpb: &Fat32Bpb,
    current_cluster: u32,
) -> Option<u32> {
    // Each FAT32 entry is 4 bytes. Compute which sector of the FAT contains
    // the entry and the byte offset within that sector.
    let fat_offset = (current_cluster as u64) * 4;
    let bps = bpb.bytes_per_sector as u64;
    let fat_sector = bpb.fat_start_sector() + fat_offset / bps;
    let offset_in_sector = (fat_offset % bps) as usize;

    let mut buf = [0u8; SECTOR_SIZE];
    if disk.read_sectors(fat_sector, 1, &mut buf).is_err() {
        return None;
    }

    if offset_in_sector + 4 > SECTOR_SIZE {
        // Entry spans a sector boundary — unlikely with 512-byte sectors and
        // 4-byte entries, but handle it defensively.
        return None;
    }

    let entry = u32::from_le_bytes([
        buf[offset_in_sector],
        buf[offset_in_sector + 1],
        buf[offset_in_sector + 2],
        buf[offset_in_sector + 3],
    ]) & 0x0FFF_FFFF; // Mask upper 4 reserved bits

    if entry >= 0x0FFF_FFF8 {
        None // End of chain
    } else if entry < 2 {
        None // Bad / free cluster
    } else {
        Some(entry)
    }
}

/// Collect all clusters in a chain starting from `start_cluster`.
fn collect_cluster_chain(
    disk: &dyn BlockDevice,
    bpb: &Fat32Bpb,
    start_cluster: u32,
) -> Vec<u32> {
    let mut chain = Vec::new();
    let mut cluster = start_cluster;

    // Guard against infinite loops (corrupt FAT).  A sane limit is the
    // total number of data clusters; fall back to 1M if we can't compute it.
    let max_clusters = (bpb.sectors_per_fat_32 as usize) * (bpb.bytes_per_sector as usize) / 4;
    let limit = if max_clusters > 0 { max_clusters } else { 1_048_576 };

    for _ in 0..limit {
        chain.push(cluster);
        match follow_cluster_chain(disk, bpb, cluster) {
            Some(next) => cluster = next,
            None => break,
        }
    }
    chain
}

// ─── Directory Entry Parsing (with LFN support) ───────────────────────────────

const ATTR_READ_ONLY: u8 = 0x01;
const ATTR_HIDDEN: u8 = 0x02;
const ATTR_SYSTEM: u8 = 0x04;
const ATTR_VOLUME_ID: u8 = 0x08;
const ATTR_DIRECTORY: u8 = 0x10;
const _ATTR_ARCHIVE: u8 = 0x20;
const ATTR_LFN: u8 = 0x0F;

#[derive(Debug, Clone)]
struct DirEntry {
    name: String,
    is_dir: bool,
    first_cluster: u32,
    file_size: u32,
    entry_cluster: u32,
    entry_offset: usize,
}

// ─── LFN helpers ─────────────────────────────────────────────────────────────

/// Compute the VFAT checksum over an 8.3 short name (11 bytes).
#[allow(dead_code)]
fn lfn_checksum(short_name: &[u8; 11]) -> u8 {
    let mut sum = 0u8;
    for i in 0..11 {
        sum = ((sum & 1) << 7) | (sum >> 1);
        sum = sum.wrapping_add(short_name[i]);
    }
    sum
}

/// Decode a sequence of LFN directory entries (encountered in forward order,
/// i.e. last-part first) into a UTF-8 String.
fn decode_lfn_name(lfn_entries: &[[u8; 32]]) -> Option<String> {
    let mut chars: Vec<u16> = Vec::new();
    for entry in lfn_entries.iter().rev() {
        for i in 0..5 {
            let c = u16::from_le_bytes([entry[1 + i * 2], entry[2 + i * 2]]);
            if c == 0x0000 || c == 0xFFFF {
                break;
            }
            chars.push(c);
        }
        for i in 0..6 {
            let c = u16::from_le_bytes([entry[14 + i * 2], entry[15 + i * 2]]);
            if c == 0x0000 || c == 0xFFFF {
                break;
            }
            chars.push(c);
        }
        for i in 0..2 {
            let c = u16::from_le_bytes([entry[28 + i * 2], entry[29 + i * 2]]);
            if c == 0x0000 || c == 0xFFFF {
                break;
            }
            chars.push(c);
        }
    }
    if chars.is_empty() {
        return None;
    }
    let mut name = String::new();
    for &c in &chars {
        if c <= 0x7F {
            name.push(c as u8 as char);
        } else {
            name.push('\u{FFFD}');
        }
    }
    Some(name)
}

/// Read all directory entries from a cluster chain (LFN-aware).
fn read_directory(
    disk: &dyn BlockDevice,
    bpb: &Fat32Bpb,
    dir_cluster: u32,
) -> Vec<DirEntry> {
    let chain = collect_cluster_chain(disk, bpb, dir_cluster);
    let bytes_per_cluster =
        (bpb.sectors_per_cluster as usize) * (bpb.bytes_per_sector as usize);
    let mut entries = Vec::new();
    let mut lfn_buf: Vec<[u8; 32]> = Vec::new();

    for &cluster in &chain {
        let sector = bpb.cluster_to_sector(cluster);
        let mut cluster_buf = vec![0u8; bytes_per_cluster];

        if disk
            .read_sectors(sector, bpb.sectors_per_cluster as u32, &mut cluster_buf)
            .is_err()
        {
            break;
        }

        let mut offset = 0;
        while offset + 32 <= bytes_per_cluster {
            let raw = &cluster_buf[offset..offset + 32];
            let first_byte = raw[0];

            if first_byte == 0x00 {
                return entries;
            }

            if first_byte == 0xE5 {
                lfn_buf.clear();
                offset += 32;
                continue;
            }

            let attr = raw[11];

            if attr == ATTR_LFN {
                let mut buf = [0u8; 32];
                buf.copy_from_slice(raw);
                lfn_buf.push(buf);
                offset += 32;
                continue;
            }

            if (attr & ATTR_VOLUME_ID) != 0 {
                lfn_buf.clear();
                offset += 32;
                continue;
            }

            // Build 8.3 short name
            let name_str = core::str::from_utf8(&raw[0..8])
                .unwrap_or("")
                .trim_end();
            let ext_str = core::str::from_utf8(&raw[8..11])
                .unwrap_or("")
                .trim_end();

            let short_name = if ext_str.is_empty() {
                String::from(name_str)
            } else {
                let mut s = String::from(name_str);
                s.push('.');
                s.push_str(ext_str);
                s
            };

            // Use LFN if available, fall back to lowercase short name
            let full_name = if !lfn_buf.is_empty() {
                decode_lfn_name(&lfn_buf).unwrap_or_else(|| short_name.to_ascii_lowercase())
            } else {
                short_name.to_ascii_lowercase()
            };

            let is_dir = (attr & ATTR_DIRECTORY) != 0;

            let cluster_hi = u16::from_le_bytes([raw[20], raw[21]]);
            let cluster_lo = u16::from_le_bytes([raw[26], raw[27]]);
            let first_cluster = ((cluster_hi as u32) << 16) | (cluster_lo as u32);

            let file_size = u32::from_le_bytes([raw[28], raw[29], raw[30], raw[31]]);

            entries.push(DirEntry {
                name: full_name,
                is_dir,
                first_cluster,
                file_size,
                entry_cluster: cluster,
                entry_offset: offset,
            });

            lfn_buf.clear();
            offset += 32;
        }
    }

    entries
}

// ─── Fat32File — VFS File implementation ─────────────────────────────────────

/// A file backed by a FAT32 cluster chain on a block device with write support.
pub struct Fat32File {
    /// Reference to the underlying block device.
    disk: Arc<dyn BlockDevice>,
    /// First cluster of the file's data.
    start_cluster: u32,
    /// Size of the file in bytes (from the directory entry).
    file_size: u32,
    /// Parsed BPB data for computing sector addresses.
    bpb: Fat32Bpb,
    /// Location of directory entry on disk.
    entry_cluster: u32,
    /// Byte offset of the directory entry within entry_cluster.
    entry_offset: usize,
}

impl Fat32File {
    /// Create a new `Fat32File`.
    fn new(
        disk: Arc<dyn BlockDevice>,
        start_cluster: u32,
        file_size: u32,
        bpb: Fat32Bpb,
        entry_cluster: u32,
        entry_offset: usize,
    ) -> Self {
        Self {
            disk,
            start_cluster,
            file_size,
            bpb,
            entry_cluster,
            entry_offset,
        }
    }
}

impl File for Fat32File {
    fn read(&self, buf: &mut [u8], offset: usize) -> Result<usize, AbiError> {
        let size = self.file_size as usize;
        if offset >= size {
            return Ok(0);
        }

        let available = size - offset;
        let to_read = available.min(buf.len());
        if to_read == 0 {
            return Ok(0);
        }

        let bytes_per_cluster =
            (self.bpb.sectors_per_cluster as usize) * (self.bpb.bytes_per_sector as usize);

        // Determine which cluster in the chain the offset falls into
        let cluster_index = offset / bytes_per_cluster;
        let offset_in_cluster = offset % bytes_per_cluster;

        // Walk the cluster chain to the target cluster
        let chain = if self.start_cluster == 0 {
            Vec::new()
        } else {
            collect_cluster_chain(&*self.disk, &self.bpb, self.start_cluster)
        };
        if cluster_index >= chain.len() {
            return Ok(0);
        }

        let mut bytes_copied = 0;
        let mut chain_idx = cluster_index;
        let mut intra_offset = offset_in_cluster;

        let mut cluster_buf = vec![0u8; bytes_per_cluster];

        while bytes_copied < to_read && chain_idx < chain.len() {
            let cluster = chain[chain_idx];
            let sector = self.bpb.cluster_to_sector(cluster);

            self.disk
                .read_sectors(
                    sector,
                    self.bpb.sectors_per_cluster as u32,
                    &mut cluster_buf,
                )
                .map_err(|_| AbiError::Other("FAT32 read I/O error"))?;

            let copy_from = intra_offset;
            let remaining_in_cluster = bytes_per_cluster - copy_from;
            let copy_len = remaining_in_cluster.min(to_read - bytes_copied);

            buf[bytes_copied..bytes_copied + copy_len]
                .copy_from_slice(&cluster_buf[copy_from..copy_from + copy_len]);

            bytes_copied += copy_len;
            chain_idx += 1;
            intra_offset = 0; // subsequent clusters start at offset 0
        }

        Ok(bytes_copied)
    }

    fn write(&mut self, buf: &[u8], offset: usize) -> Result<usize, AbiError> {
        crate::println!("DEBUG: file_size={}, offset={}, buf_len={}", self.file_size, offset, buf.len());
        crate::println!("DEBUG: bpb: bytes_per_sector={}, sectors_per_cluster={}", self.bpb.bytes_per_sector, self.bpb.sectors_per_cluster);
        let new_size = self.file_size.max((offset + buf.len()) as u32);
        let bytes_per_cluster =
            (self.bpb.sectors_per_cluster as usize) * (self.bpb.bytes_per_sector as usize);

        let clusters_needed = if new_size == 0 {
            0
        } else {
            (new_size as usize + bytes_per_cluster - 1) / bytes_per_cluster
        };

        let mut chain = if self.start_cluster == 0 {
            Vec::new()
        } else {
            collect_cluster_chain(&*self.disk, &self.bpb, self.start_cluster)
        };

        let old_start_cluster = self.start_cluster;

        if clusters_needed > chain.len() {
            let mut last_cluster = chain.last().cloned();
            let to_alloc = clusters_needed - chain.len();
            for _ in 0..to_alloc {
                let new_cl = allocate_cluster(&*self.disk, &self.bpb, last_cluster)
                    .ok_or(AbiError::Other("Disk Full"))?;
                if self.start_cluster == 0 {
                    self.start_cluster = new_cl;
                }
                chain.push(new_cl);
                last_cluster = Some(new_cl);
            }
        }

        let mut bytes_written = 0;
        let mut chain_idx = offset / bytes_per_cluster;
        let mut intra_offset = offset % bytes_per_cluster;
        let mut cluster_buf = vec![0u8; bytes_per_cluster];

        while bytes_written < buf.len() && chain_idx < chain.len() {
            let cluster = chain[chain_idx];
            let sector = self.bpb.cluster_to_sector(cluster);

            self.disk
                .read_sectors(
                    sector,
                    self.bpb.sectors_per_cluster as u32,
                    &mut cluster_buf,
                )
                .map_err(|_| AbiError::Other("FAT32 write I/O read error"))?;

            let copy_to = intra_offset;
            let remaining_in_cluster = bytes_per_cluster - copy_to;
            let copy_len = remaining_in_cluster.min(buf.len() - bytes_written);

            cluster_buf[copy_to..copy_to + copy_len]
                .copy_from_slice(&buf[bytes_written..bytes_written + copy_len]);

            self.disk
                .write_sectors(
                    sector,
                    self.bpb.sectors_per_cluster as u32,
                    &cluster_buf,
                )
                .map_err(|_| AbiError::Other("FAT32 write I/O write error"))?;

            bytes_written += copy_len;
            chain_idx += 1;
            intra_offset = 0;
        }

        if new_size != self.file_size || self.start_cluster != old_start_cluster {
            let dir_sector = self.bpb.cluster_to_sector(self.entry_cluster);
            let mut dir_buf = vec![0u8; bytes_per_cluster];
            self.disk
                .read_sectors(
                    dir_sector,
                    self.bpb.sectors_per_cluster as u32,
                    &mut dir_buf,
                )
                .map_err(|_| AbiError::Other("FAT32 write directory I/O read error"))?;

            let entry = &mut dir_buf[self.entry_offset..self.entry_offset + 32];

            let hi = ((self.start_cluster >> 16) & 0xFFFF) as u16;
            let lo = (self.start_cluster & 0xFFFF) as u16;
            entry[20..22].copy_from_slice(&hi.to_le_bytes());
            entry[26..28].copy_from_slice(&lo.to_le_bytes());
            entry[28..32].copy_from_slice(&new_size.to_le_bytes());

            self.disk
                .write_sectors(
                    dir_sector,
                    self.bpb.sectors_per_cluster as u32,
                    &mut dir_buf,
                )
                .map_err(|_| AbiError::Other("FAT32 write directory update error"))?;

            self.file_size = new_size;
        }

        Ok(bytes_written)
    }

    fn file_type(&self) -> FileType {
        FileType::Regular
    }

    fn size(&self) -> usize {
        self.file_size as usize
    }
}

// ─── MBR Partition Parsing ───────────────────────────────────────────────────

/// Scan the MBR partition table and return the (start_sector, size_sectors)
/// of the first FAT32 partition found.
///
/// FAT32 partition type codes:
/// - `0x0B` — FAT32 (CHS addressing)
/// - `0x0C` — FAT32 with LBA
pub fn find_fat32_partition(disk: &dyn BlockDevice) -> Option<(u64, u64)> {
    let mut mbr = [0u8; SECTOR_SIZE];
    if disk.read_sectors(0, 1, &mut mbr).is_err() {
        return None;
    }

    // Verify MBR signature
    if mbr[0x1FE] != 0x55 || mbr[0x1FF] != 0xAA {
        return None;
    }

    // Partition table starts at offset 446, four 16-byte entries
    for i in 0..4 {
        let base = 446 + i * 16;
        let ptype = mbr[base + 4];

        if ptype == 0x0B || ptype == 0x0C {
            let lba_start = u32::from_le_bytes([
                mbr[base + 8],
                mbr[base + 9],
                mbr[base + 10],
                mbr[base + 11],
            ]);
            let lba_size = u32::from_le_bytes([
                mbr[base + 12],
                mbr[base + 13],
                mbr[base + 14],
                mbr[base + 15],
            ]);

            if lba_start > 0 && lba_size > 0 {
                return Some((lba_start as u64, lba_size as u64));
            }
        }
    }

    None
}

// ─── Recursive Directory Walk & VFS Mount ────────────────────────────────────

/// Recursively walk a FAT32 directory and mount each regular file into the VFS.
fn walk_and_mount(
    disk: &Arc<dyn BlockDevice>,
    bpb: &Fat32Bpb,
    dir_cluster: u32,
    vfs_prefix: &str,
) {
    crate::println!("DEBUG walk_and_mount: dir_cluster={}, vfs_prefix={}", dir_cluster, vfs_prefix);
    let entries = read_directory(&**disk, bpb, dir_cluster);
    crate::println!("DEBUG walk_and_mount: found {} entries in cluster {}", entries.len(), dir_cluster);
    let mut vfs = VFS.write();

    for entry in &entries {
        crate::println!("DEBUG walk_and_mount: entry={}, is_dir={}, cluster={}", entry.name, entry.is_dir, entry.first_cluster);
        // Skip the `.` and `..` directory entries to avoid infinite recursion
        if entry.name == "." || entry.name == ".." {
            continue;
        }

        let path = if vfs_prefix == "/" {
            alloc::format!("/{}", entry.name)
        } else {
            alloc::format!("{}/{}", vfs_prefix, entry.name)
        };

        if entry.is_dir {
            // Create a directory marker in the VFS
            vfs.mkdir(&path);
            // Release VFS lock before recursing to avoid deadlock
            drop(vfs);
            walk_and_mount(disk, bpb, entry.first_cluster, &path);
            vfs = VFS.write();
        } else {
            // Mount the file in VFS (even if it's 0-sized)
            let fat_file = Fat32File::new(
                disk.clone(),
                entry.first_cluster,
                entry.file_size,
                *bpb,
                entry.entry_cluster,
                entry.entry_offset,
            );
            vfs.mount(&path, Arc::new(Mutex::new(fat_file)));
        }
    }
    crate::println!("DEBUG walk_and_mount finished: dir_cluster={}", dir_cluster);
}

// ─── Public Mount Entry Point ────────────────────────────────────────────────

pub struct Fat32Fs {
    pub disk: Arc<dyn BlockDevice>,
    pub bpb: Fat32Bpb,
    pub mount_point: String,
}

lazy_static! {
    pub static ref FAT32: Mutex<Option<Arc<Mutex<Fat32Fs>>>> = Mutex::new(None);
}

/// Mount a FAT32 filesystem from a block device onto the VFS.
pub fn mount_fat32(
    disk: Arc<dyn BlockDevice>,
    partition_start: u64,
    mount_point: &str,
) -> Result<(), &'static str> {
    // 1. Read the FAT32 boot sector (first sector of the partition)
    let mut boot_sector = [0u8; SECTOR_SIZE];
    disk.read_sectors(partition_start, 1, &mut boot_sector)
        .map_err(|_| "FAT32: failed to read boot sector")?;

    // 2. Parse the BPB
    let bpb = Fat32Bpb::parse(&boot_sector, partition_start).map_err(|e| match e {
        Fat32Error::BadSignature => "FAT32: bad boot sector signature (expected 0xAA55)",
        Fat32Error::InvalidBpb(msg) => msg,
        Fat32Error::IoError => "FAT32: I/O error reading boot sector",
        Fat32Error::NotFat32 => "FAT32: partition is not FAT32",
    })?;

    // 3. Basic sanity checks
    if bpb.sectors_per_fat_32 == 0 {
        return Err("FAT32: sectors_per_fat is zero — not a FAT32 volume");
    }
    if bpb.root_cluster < 2 {
        return Err("FAT32: root cluster number is invalid");
    }

    // 4. Log detected parameters
    crate::println!(
        " ~ FAT32 BPB: bps={} spc={} reserved={} fats={} spf={} root_cl={}",
        bpb.bytes_per_sector,
        bpb.sectors_per_cluster,
        bpb.reserved_sectors,
        bpb.num_fats,
        bpb.sectors_per_fat_32,
        bpb.root_cluster
    );

    // 5. Walk the root directory and mount all files
    walk_and_mount(&disk, &bpb, bpb.root_cluster, mount_point);

    let fs = Arc::new(Mutex::new(Fat32Fs {
        disk: disk.clone(),
        bpb,
        mount_point: String::from(mount_point),
    }));
    *FAT32.lock() = Some(fs);

    Ok(())
}

// ─── Helpers for write, allocation, LFN, unlink, and rename ─────────────────

fn write_fat_entry(
    disk: &dyn BlockDevice,
    bpb: &Fat32Bpb,
    current_cluster: u32,
    next_cluster: u32,
) -> Result<(), Fat32Error> {
    let fat_offset = (current_cluster as u64) * 4;
    let bps = bpb.bytes_per_sector as u64;
    let fat_sector = bpb.fat_start_sector() + fat_offset / bps;
    let offset_in_sector = (fat_offset % bps) as usize;

    let mut buf = [0u8; SECTOR_SIZE];
    disk.read_sectors(fat_sector, 1, &mut buf)
        .map_err(|_| Fat32Error::IoError)?;

    let old_entry = u32::from_le_bytes([
        buf[offset_in_sector],
        buf[offset_in_sector + 1],
        buf[offset_in_sector + 2],
        buf[offset_in_sector + 3],
    ]);
    let new_entry = (old_entry & 0xF000_0000) | (next_cluster & 0x0FFF_FFFF);
    buf[offset_in_sector..offset_in_sector + 4].copy_from_slice(&new_entry.to_le_bytes());

    for fat_idx in 0..bpb.num_fats {
        let sector = fat_sector + (fat_idx as u64) * (bpb.sectors_per_fat_32 as u64);
        disk.write_sectors(sector, 1, &buf)
            .map_err(|_| Fat32Error::IoError)?;
    }

    Ok(())
}

fn find_free_cluster(disk: &dyn BlockDevice, bpb: &Fat32Bpb) -> Option<u32> {
    let max_clusters = (bpb.sectors_per_fat_32 as usize) * (bpb.bytes_per_sector as usize) / 4;
    let bps = bpb.bytes_per_sector as usize;
    let sectors_per_fat = bpb.sectors_per_fat_32;
    let fat_start = bpb.fat_start_sector();

    let mut buf = [0u8; SECTOR_SIZE];

    for sec_idx in 0..sectors_per_fat {
        let sector = fat_start + sec_idx as u64;
        if disk.read_sectors(sector, 1, &mut buf).is_err() {
            continue;
        }

        for offset in (0..bps).step_by(4) {
            let cluster = (sec_idx as usize * bps + offset) / 4;
            if cluster < 2 || cluster >= max_clusters {
                continue;
            }
            let entry = u32::from_le_bytes([
                buf[offset],
                buf[offset + 1],
                buf[offset + 2],
                buf[offset + 3],
            ]) & 0x0FFF_FFFF;

            if entry == 0 {
                return Some(cluster as u32);
            }
        }
    }
    None
}

fn allocate_cluster(
    disk: &dyn BlockDevice,
    bpb: &Fat32Bpb,
    prev_cluster: Option<u32>,
) -> Option<u32> {
    let new_cluster = find_free_cluster(disk, bpb)?;

    if write_fat_entry(disk, bpb, new_cluster, 0x0FFF_FFFF).is_err() {
        return None;
    }

    if let Some(prev) = prev_cluster {
        if write_fat_entry(disk, bpb, prev, new_cluster).is_err() {
            let _ = write_fat_entry(disk, bpb, new_cluster, 0);
            return None;
        }
    }

    let start_sector = bpb.cluster_to_sector(new_cluster);
    let bytes_per_cluster =
        (bpb.sectors_per_cluster as usize) * (bpb.bytes_per_sector as usize);
    let zero_buf = vec![0u8; bytes_per_cluster];
    if disk
        .write_sectors(start_sector, bpb.sectors_per_cluster as u32, &zero_buf)
        .is_err()
    {
        if let Some(prev) = prev_cluster {
            let _ = write_fat_entry(disk, bpb, prev, 0x0FFF_FFFF);
        }
        let _ = write_fat_entry(disk, bpb, new_cluster, 0);
        return None;
    }

    Some(new_cluster)
}

struct DirSlot {
    cluster: u32,
    offset: usize,
}

fn find_or_allocate_dir_slot(
    disk: &dyn BlockDevice,
    bpb: &Fat32Bpb,
    dir_cluster: u32,
) -> Option<DirSlot> {
    let chain = collect_cluster_chain(disk, bpb, dir_cluster);
    let bytes_per_cluster =
        (bpb.sectors_per_cluster as usize) * (bpb.bytes_per_sector as usize);

    let mut last_cluster = dir_cluster;

    for &cluster in &chain {
        last_cluster = cluster;
        let sector = bpb.cluster_to_sector(cluster);
        let mut cluster_buf = vec![0u8; bytes_per_cluster];

        if disk
            .read_sectors(sector, bpb.sectors_per_cluster as u32, &mut cluster_buf)
            .is_err()
        {
            return None;
        }

        let mut offset = 0;
        while offset + 32 <= bytes_per_cluster {
            let first_byte = cluster_buf[offset];
            if first_byte == 0x00 || first_byte == 0xE5 {
                if first_byte == 0x00 && offset + 64 <= bytes_per_cluster {
                    cluster_buf[offset + 32] = 0x00;
                    if disk
                        .write_sectors(sector, bpb.sectors_per_cluster as u32, &cluster_buf)
                        .is_err()
                    {
                        return None;
                    }
                }
                return Some(DirSlot { cluster, offset });
            }
            offset += 32;
        }
    }

    let new_cluster = allocate_cluster(disk, bpb, Some(last_cluster))?;
    Some(DirSlot {
        cluster: new_cluster,
        offset: 0,
    })
}

fn to_short_name(name: &str) -> Option<[u8; 11]> {
    let mut res = [b' '; 11];

    let parts: Vec<&str> = name.split('.').collect();
    if parts.is_empty() {
        return None;
    }

    let base = parts[0].to_ascii_uppercase();
    let ext = if parts.len() > 1 {
        parts[parts.len() - 1].to_ascii_uppercase()
    } else {
        String::new()
    };

    let base_bytes = base.as_bytes();
    let ext_bytes = ext.as_bytes();

    let base_len = base_bytes.len().min(8);
    let ext_len = ext_bytes.len().min(3);

    res[0..base_len].copy_from_slice(&base_bytes[..base_len]);
    res[8..8 + ext_len].copy_from_slice(&ext_bytes[..ext_len]);

    Some(res)
}

fn write_dir_entry(
    disk: &dyn BlockDevice,
    bpb: &Fat32Bpb,
    slot: &DirSlot,
    short_name: [u8; 11],
    attr: u8,
    first_cluster: u32,
    file_size: u32,
) -> Result<(), Fat32Error> {
    let sector = bpb.cluster_to_sector(slot.cluster);
    let bytes_per_cluster =
        (bpb.sectors_per_cluster as usize) * (bpb.bytes_per_sector as usize);
    let mut cluster_buf = vec![0u8; bytes_per_cluster];

    disk.read_sectors(sector, bpb.sectors_per_cluster as u32, &mut cluster_buf)
        .map_err(|_| Fat32Error::IoError)?;

    let entry = &mut cluster_buf[slot.offset..slot.offset + 32];

    entry[0..11].copy_from_slice(&short_name);
    entry[11] = attr;
    entry[12] = 0;
    entry[13] = 0;
    entry[14..16].copy_from_slice(&0u16.to_le_bytes());
    entry[16..18].copy_from_slice(&0u16.to_le_bytes());
    entry[18..20].copy_from_slice(&0u16.to_le_bytes());

    let hi = ((first_cluster >> 16) & 0xFFFF) as u16;
    let lo = (first_cluster & 0xFFFF) as u16;

    entry[20..22].copy_from_slice(&hi.to_le_bytes());
    entry[22..24].copy_from_slice(&0u16.to_le_bytes());
    entry[24..26].copy_from_slice(&0u16.to_le_bytes());
    entry[26..28].copy_from_slice(&lo.to_le_bytes());
    entry[28..32].copy_from_slice(&file_size.to_le_bytes());

    disk.write_sectors(sector, bpb.sectors_per_cluster as u32, &cluster_buf)
        .map_err(|_| Fat32Error::IoError)?;

    Ok(())
}

fn find_dir_entry<'a>(
    disk: &dyn BlockDevice,
    bpb: &Fat32Bpb,
    dir_cluster: u32,
    name: &str,
) -> Option<(u32, usize, u32, u32)> {
    let chain = collect_cluster_chain(disk, bpb, dir_cluster);
    let bytes_per_cluster =
        (bpb.sectors_per_cluster as usize) * (bpb.bytes_per_sector as usize);
    let mut lfn_buf: Vec<[u8; 32]> = Vec::new();

    for &cluster in &chain {
        let sector = bpb.cluster_to_sector(cluster);
        let mut cluster_buf = vec![0u8; bytes_per_cluster];

        if disk
            .read_sectors(sector, bpb.sectors_per_cluster as u32, &mut cluster_buf)
            .is_err()
        {
            break;
        }

        let mut offset = 0;
        while offset + 32 <= bytes_per_cluster {
            let raw = &cluster_buf[offset..offset + 32];
            let first_byte = raw[0];

            if first_byte == 0x00 {
                return None;
            }

            if first_byte == 0xE5 {
                lfn_buf.clear();
                offset += 32;
                continue;
            }

            let attr = raw[11];

            if attr == ATTR_LFN {
                let mut buf = [0u8; 32];
                buf.copy_from_slice(raw);
                lfn_buf.push(buf);
                offset += 32;
                continue;
            }

            if (attr & ATTR_VOLUME_ID) != 0 {
                lfn_buf.clear();
                offset += 32;
                continue;
            }

            let name_str = core::str::from_utf8(&raw[0..8])
                .unwrap_or("")
                .trim_end();
            let ext_str = core::str::from_utf8(&raw[8..11])
                .unwrap_or("")
                .trim_end();

            let short_name = if ext_str.is_empty() {
                alloc::string::String::from(name_str)
            } else {
                let mut s = alloc::string::String::from(name_str);
                s.push('.');
                s.push_str(ext_str);
                s
            };

            let full_name = if !lfn_buf.is_empty() {
                decode_lfn_name(&lfn_buf).unwrap_or_else(|| short_name.to_ascii_lowercase())
            } else {
                short_name.to_ascii_lowercase()
            };

            if full_name.eq_ignore_ascii_case(name) {
                let cluster_hi = u16::from_le_bytes([raw[20], raw[21]]);
                let cluster_lo = u16::from_le_bytes([raw[26], raw[27]]);
                let first_cluster = ((cluster_hi as u32) << 16) | (cluster_lo as u32);
                let file_size = u32::from_le_bytes([raw[28], raw[29], raw[30], raw[31]]);
                return Some((cluster, offset, first_cluster, file_size));
            }

            lfn_buf.clear();
            offset += 32;
        }
    }

    None
}

fn free_cluster_chain(
    disk: &dyn BlockDevice,
    bpb: &Fat32Bpb,
    start_cluster: u32,
) -> Result<(), Fat32Error> {
    let max_clusters = (bpb.sectors_per_fat_32 as usize) * (bpb.bytes_per_sector as usize) / 4;
    let limit = if max_clusters > 0 { max_clusters } else { 1_048_576 };

    let mut cluster = start_cluster;
    for _ in 0..limit {
        if cluster < 2 {
            break;
        }
        let next = follow_cluster_chain(disk, bpb, cluster);
        write_fat_entry(disk, bpb, cluster, 0)?;
        match next {
            Some(n) if n < 0x0FFF_FFF8 => cluster = n,
            _ => break,
        }
    }
    Ok(())
}

fn resolve_parent_dir(
    disk: &dyn BlockDevice,
    bpb: &Fat32Bpb,
    mount_point: &str,
    path: &str,
) -> Result<(u32, String), &'static str> {
    if !path.starts_with(mount_point) {
        return Err("Path does not start with mount point");
    }
    let rel_path = path[mount_point.len()..].trim_start_matches('/');
    if rel_path.is_empty() {
        return Err("Invalid path (cannot create root directory)");
    }

    let parts: Vec<&str> = rel_path.split('/').collect();
    let leaf_name = String::from(*parts.last().unwrap());

    let mut current_cluster = bpb.root_cluster;

    for i in 0..parts.len() - 1 {
        let dir_name = parts[i];
        if dir_name.is_empty() {
            continue;
        }

        let entries = read_directory(disk, bpb, current_cluster);
        let mut found = false;
        for entry in entries {
            if entry.is_dir && entry.name.eq_ignore_ascii_case(dir_name) {
                current_cluster = entry.first_cluster;
                found = true;
                break;
            }
        }
        if !found {
            return Err("Parent directory not found");
        }
    }

    Ok((current_cluster, leaf_name))
}

pub fn create_file_on_disk(path: &str) -> Result<Fat32File, &'static str> {
    let fs_guard = FAT32.lock();
    let fs_arc = fs_guard.as_ref().ok_or("FAT32 filesystem not mounted")?;
    let fs = fs_arc.lock();

    let (parent_cluster, leaf_name) =
        resolve_parent_dir(&*fs.disk, &fs.bpb, &fs.mount_point, path)?;

    let short_name = to_short_name(&leaf_name).ok_or("Invalid filename for FAT32")?;

    let entries = read_directory(&*fs.disk, &fs.bpb, parent_cluster);
    for entry in entries {
        if entry.name.eq_ignore_ascii_case(&leaf_name) {
            return Err("File already exists");
        }
    }

    let slot = find_or_allocate_dir_slot(&*fs.disk, &fs.bpb, parent_cluster)
        .ok_or("Directory full / disk error")?;

    write_dir_entry(&*fs.disk, &fs.bpb, &slot, short_name, 0x20, 0, 0)
        .map_err(|_| "Failed to write directory entry")?;

    crate::println!("DEBUG create_file_on_disk: fs.bpb.bytes_per_sector={}, fs.bpb.sectors_per_cluster={}", fs.bpb.bytes_per_sector, fs.bpb.sectors_per_cluster);

    let fat_file = Fat32File {
        disk: fs.disk.clone(),
        start_cluster: 0,
        file_size: 0,
        bpb: fs.bpb,
        entry_cluster: slot.cluster,
        entry_offset: slot.offset,
    };

    crate::println!("DEBUG create_file_on_disk: fat_file.bpb.bytes_per_sector={}, fat_file.bpb.sectors_per_cluster={}", fat_file.bpb.bytes_per_sector, fat_file.bpb.sectors_per_cluster);

    Ok(fat_file)
}

pub fn rmdir_on_disk(path: &str) -> Result<(), &'static str> {
    let fs_guard = FAT32.lock();
    let fs_arc = fs_guard.as_ref().ok_or("FAT32 filesystem not mounted")?;
    let fs = fs_arc.lock();

    let (parent_cluster, leaf_name) =
        resolve_parent_dir(&*fs.disk, &fs.bpb, &fs.mount_point, path)?;

    let entry_info = find_dir_entry(&*fs.disk, &fs.bpb, parent_cluster, &leaf_name)
        .ok_or("Directory not found")?;

    let (entry_cluster, entry_offset, first_cluster, _file_size) = entry_info;

    let is_dir = read_directory(&*fs.disk, &fs.bpb, parent_cluster)
        .iter()
        .find(|e| e.name.eq_ignore_ascii_case(&leaf_name))
        .map(|e| e.is_dir)
        .unwrap_or(false);

    if !is_dir {
        return Err("Not a directory (use unlink for files)");
    }

    let dir_entries = read_directory(&*fs.disk, &fs.bpb, first_cluster);
    let non_dot_entries: Vec<_> = dir_entries
        .iter()
        .filter(|e| e.name != "." && e.name != "..")
        .collect();

    if !non_dot_entries.is_empty() {
        return Err("Directory not empty");
    }

    free_cluster_chain(&*fs.disk, &fs.bpb, first_cluster)
        .map_err(|_| "Failed to free directory cluster")?;

    let bytes_per_cluster =
        (fs.bpb.sectors_per_cluster as usize) * (fs.bpb.bytes_per_sector as usize);
    let mut cluster_buf = vec![0u8; bytes_per_cluster];

    let sector = fs.bpb.cluster_to_sector(entry_cluster);
    fs.disk
        .read_sectors(sector, fs.bpb.sectors_per_cluster as u32, &mut cluster_buf)
        .map_err(|_| "Failed to read directory sector")?;

    cluster_buf[entry_offset] = 0xE5;

    fs.disk
        .write_sectors(sector, fs.bpb.sectors_per_cluster as u32, &cluster_buf)
        .map_err(|_| "Failed to write directory sector")?;

    Ok(())
}

pub fn mkdir_on_disk(path: &str) -> Result<(), &'static str> {
    let fs_guard = FAT32.lock();
    let fs_arc = fs_guard.as_ref().ok_or("FAT32 filesystem not mounted")?;
    let fs = fs_arc.lock();

    let (parent_cluster, leaf_name) =
        resolve_parent_dir(&*fs.disk, &fs.bpb, &fs.mount_point, path)?;

    let short_name = to_short_name(&leaf_name).ok_or("Invalid directory name for FAT32")?;

    let entries = read_directory(&*fs.disk, &fs.bpb, parent_cluster);
    for entry in entries {
        if entry.name.eq_ignore_ascii_case(&leaf_name) {
            return Err("Directory/file already exists");
        }
    }

    let new_dir_cluster = allocate_cluster(&*fs.disk, &fs.bpb, None).ok_or("Disk full")?;

    let mut dot_name = [b' '; 11];
    dot_name[0] = b'.';
    write_dir_entry(
        &*fs.disk,
        &fs.bpb,
        &DirSlot {
            cluster: new_dir_cluster,
            offset: 0,
        },
        dot_name,
        ATTR_DIRECTORY,
        new_dir_cluster,
        0,
    )
    .map_err(|_| "Failed to write . entry")?;

    let mut dotdot_name = [b' '; 11];
    dotdot_name[0] = b'.';
    dotdot_name[1] = b'.';
    let parent_cluster_val = if parent_cluster == fs.bpb.root_cluster {
        0
    } else {
        parent_cluster
    };
    write_dir_entry(
        &*fs.disk,
        &fs.bpb,
        &DirSlot {
            cluster: new_dir_cluster,
            offset: 32,
        },
        dotdot_name,
        ATTR_DIRECTORY,
        parent_cluster_val,
        0,
    )
    .map_err(|_| "Failed to write .. entry")?;

    let slot = find_or_allocate_dir_slot(&*fs.disk, &fs.bpb, parent_cluster)
        .ok_or("Directory full")?;

    write_dir_entry(
        &*fs.disk,
        &fs.bpb,
        &slot,
        short_name,
        ATTR_DIRECTORY,
        new_dir_cluster,
        0,
    )
    .map_err(|_| "Failed to write directory entry in parent")?;

    Ok(())
}

pub fn unlink_on_disk(path: &str) -> Result<(), &'static str> {
    let fs_guard = FAT32.lock();
    let fs_arc = fs_guard.as_ref().ok_or("FAT32 filesystem not mounted")?;
    let fs = fs_arc.lock();

    let (parent_cluster, leaf_name) =
        resolve_parent_dir(&*fs.disk, &fs.bpb, &fs.mount_point, path)?;

    let entry_info = find_dir_entry(&*fs.disk, &fs.bpb, parent_cluster, &leaf_name)
        .ok_or("File not found")?;

    let (entry_cluster, entry_offset, first_cluster, _file_size) = entry_info;

    let is_dir = read_directory(&*fs.disk, &fs.bpb, parent_cluster)
        .iter()
        .find(|e| e.name.eq_ignore_ascii_case(&leaf_name))
        .map(|e| e.is_dir)
        .unwrap_or(false);

    if is_dir {
        return Err("Cannot unlink a directory (use rmdir)");
    }

    free_cluster_chain(&*fs.disk, &fs.bpb, first_cluster)
        .map_err(|_| "Failed to free cluster chain")?;

    let bytes_per_cluster =
        (fs.bpb.sectors_per_cluster as usize) * (fs.bpb.bytes_per_sector as usize);
    let mut cluster_buf = vec![0u8; bytes_per_cluster];

    let sector = fs.bpb.cluster_to_sector(entry_cluster);
    fs.disk
        .read_sectors(sector, fs.bpb.sectors_per_cluster as u32, &mut cluster_buf)
        .map_err(|_| "Failed to read directory sector")?;

    cluster_buf[entry_offset] = 0xE5;

    fs.disk
        .write_sectors(sector, fs.bpb.sectors_per_cluster as u32, &cluster_buf)
        .map_err(|_| "Failed to write directory sector")?;

    Ok(())
}

pub fn rename_on_disk(_old: &str, _new: &str) -> Result<(), &'static str> {
    Err("rename is not supported for FAT32 yet")
}

pub fn truncate_on_disk(path: &str, new_size: usize) -> Result<(), &'static str> {
    let fs_guard = FAT32.lock();
    let fs_arc = fs_guard.as_ref().ok_or("FAT32 filesystem not mounted")?;
    let fs = fs_arc.lock();

    let (parent_cluster, leaf_name) =
        resolve_parent_dir(&*fs.disk, &fs.bpb, &fs.mount_point, path)?;

    let entry_info = find_dir_entry(&*fs.disk, &fs.bpb, parent_cluster, &leaf_name)
        .ok_or("File not found")?;

    let (entry_cluster, entry_offset, first_cluster, _file_size) = entry_info;

    if new_size == 0 {
        free_cluster_chain(&*fs.disk, &fs.bpb, first_cluster)
            .map_err(|_| "Failed to free cluster chain")?;

        let bytes_per_cluster =
            (fs.bpb.sectors_per_cluster as usize) * (fs.bpb.bytes_per_sector as usize);
        let mut cluster_buf = vec![0u8; bytes_per_cluster];
        let sector = fs.bpb.cluster_to_sector(entry_cluster);
        fs.disk
            .read_sectors(sector, fs.bpb.sectors_per_cluster as u32, &mut cluster_buf)
            .map_err(|_| "Failed to read directory sector")?;
        let hi: u16 = 0;
        let lo: u16 = 0;
        cluster_buf[entry_offset + 20..entry_offset + 22].copy_from_slice(&hi.to_le_bytes());
        cluster_buf[entry_offset + 26..entry_offset + 28].copy_from_slice(&lo.to_le_bytes());
        cluster_buf[entry_offset + 28..entry_offset + 32].copy_from_slice(&0u32.to_le_bytes());
        fs.disk
            .write_sectors(sector, fs.bpb.sectors_per_cluster as u32, &cluster_buf)
            .map_err(|_| "Failed to write directory sector")?;
        return Ok(());
    }

    Err("truncate to non-zero is not supported for FAT32 yet")
}

// ─── Trait helpers ───────────────────────────────────────────────────────────


/// Extension trait to convert ASCII bytes to lowercase in-place.
trait AsciiLowercase {
    fn to_ascii_lowercase(&self) -> Self;
}

impl AsciiLowercase for String {
    fn to_ascii_lowercase(&self) -> String {
        let mut s = self.clone();
        // SAFETY: we only modify ASCII bytes in-place, which is always valid UTF-8.
        unsafe {
            for byte in s.as_bytes_mut() {
                if *byte >= b'A' && *byte <= b'Z' {
                    *byte += 32;
                }
            }
        }
        s
    }
}

// Suppress warnings for constants used in attribute checking
#[allow(dead_code)]
const _ATTR_CONSTS_USED: [u8; 4] = [ATTR_READ_ONLY, ATTR_HIDDEN, ATTR_SYSTEM, ATTR_LFN];
