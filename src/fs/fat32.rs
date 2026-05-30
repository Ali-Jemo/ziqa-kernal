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
struct Fat32Bpb {
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

// ─── Directory Entry Parsing ─────────────────────────────────────────────────

/// Attributes byte constants.
const ATTR_READ_ONLY: u8 = 0x01;
const ATTR_HIDDEN: u8 = 0x02;
const ATTR_SYSTEM: u8 = 0x04;
const ATTR_VOLUME_ID: u8 = 0x08;
const ATTR_DIRECTORY: u8 = 0x10;
const _ATTR_ARCHIVE: u8 = 0x20;
const ATTR_LFN: u8 = 0x0F; // LFN marker (all four lower bits set)

/// A single parsed short-name directory entry.
#[derive(Debug, Clone)]
struct DirEntry {
    name: String,
    is_dir: bool,
    first_cluster: u32,
    file_size: u32,
}

/// Parse a single 32-byte directory entry.
///
/// Returns `None` for entries that should be skipped (free, deleted, LFN,
/// volume label) or if the end-of-directory sentinel (0x00) is hit.
/// Returns `Some(None)` for end-of-directory.
fn parse_dir_entry(raw: &[u8]) -> Option<Option<DirEntry>> {
    if raw.len() < 32 {
        return Some(None); // malformed → treat as end
    }

    let first_byte = raw[0];
    if first_byte == 0x00 {
        // End of directory
        return Some(None);
    }
    if first_byte == 0xE5 {
        // Deleted entry — skip
        return None;
    }

    let attr = raw[11];

    // Skip LFN entries and volume labels
    if attr == ATTR_LFN || (attr & ATTR_VOLUME_ID) != 0 {
        return None;
    }

    // Build 8.3 short name
    let name_part = &raw[0..8];
    let ext_part = &raw[8..11];

    // Trim trailing spaces from name and extension
    let name_str = core::str::from_utf8(name_part)
        .unwrap_or("")
        .trim_end();
    let ext_str = core::str::from_utf8(ext_part)
        .unwrap_or("")
        .trim_end();

    let full_name = if ext_str.is_empty() {
        String::from(name_str)
    } else {
        let mut s = String::from(name_str);
        s.push('.');
        s.push_str(ext_str);
        s
    };

    // Convert name to lowercase for more natural VFS paths
    let full_name = full_name.to_ascii_lowercase();

    let is_dir = (attr & ATTR_DIRECTORY) != 0;

    let cluster_hi = u16::from_le_bytes([raw[20], raw[21]]);
    let cluster_lo = u16::from_le_bytes([raw[26], raw[27]]);
    let first_cluster = ((cluster_hi as u32) << 16) | (cluster_lo as u32);

    let file_size = u32::from_le_bytes([raw[28], raw[29], raw[30], raw[31]]);

    Some(Some(DirEntry {
        name: full_name,
        is_dir,
        first_cluster,
        file_size,
    }))
}

/// Read all directory entries from a cluster chain.
fn read_directory(
    disk: &dyn BlockDevice,
    bpb: &Fat32Bpb,
    dir_cluster: u32,
) -> Vec<DirEntry> {
    let chain = collect_cluster_chain(disk, bpb, dir_cluster);
    let bytes_per_cluster =
        (bpb.sectors_per_cluster as usize) * (bpb.bytes_per_sector as usize);
    let mut entries = Vec::new();

    for &cluster in &chain {
        let sector = bpb.cluster_to_sector(cluster);
        let mut cluster_buf = vec![0u8; bytes_per_cluster];

        // Read all sectors of this cluster
        if disk
            .read_sectors(sector, bpb.sectors_per_cluster as u32, &mut cluster_buf)
            .is_err()
        {
            break;
        }

        // Walk 32-byte entries within the cluster
        let mut offset = 0;
        while offset + 32 <= bytes_per_cluster {
            match parse_dir_entry(&cluster_buf[offset..offset + 32]) {
                Some(None) => return entries, // end of directory
                Some(Some(entry)) => entries.push(entry),
                None => {} // skip (deleted / LFN / volume label)
            }
            offset += 32;
        }
    }

    entries
}

// ─── Fat32File — VFS File implementation ─────────────────────────────────────

/// A read-only file backed by a FAT32 cluster chain on a block device.
///
/// Each `read()` call follows the cluster chain from the beginning to locate
/// the requested offset.  This is O(n) in chain length but simple and correct;
/// future work could cache the chain or use an extent map.
pub struct Fat32File {
    /// Reference to the underlying block device.
    disk: Arc<dyn BlockDevice>,
    /// First cluster of the file's data.
    start_cluster: u32,
    /// Size of the file in bytes (from the directory entry).
    file_size: u32,
    /// Parsed BPB data for computing sector addresses.
    bpb: Fat32Bpb,
}

impl Fat32File {
    /// Create a new `Fat32File`.
    fn new(
        disk: Arc<dyn BlockDevice>,
        start_cluster: u32,
        file_size: u32,
        bpb: Fat32Bpb,
    ) -> Self {
        Self {
            disk,
            start_cluster,
            file_size,
            bpb,
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
        let chain = collect_cluster_chain(&*self.disk, &self.bpb, self.start_cluster);
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

    fn write(&mut self, _buf: &[u8], _offset: usize) -> Result<usize, AbiError> {
        Err(AbiError::Other("FAT32 is read-only"))
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
    let entries = read_directory(&**disk, bpb, dir_cluster);
    let mut vfs = VFS.write();

    for entry in &entries {
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
            // Only mount files that have a valid cluster and non-zero size
            // (zero-size files are valid FAT32 entries but not useful to mount)
            if entry.first_cluster >= 2 {
                let fat_file = Fat32File::new(
                    disk.clone(),
                    entry.first_cluster,
                    entry.file_size,
                    *bpb,
                );
                vfs.mount(&path, Arc::new(Mutex::new(fat_file)));
            }
        }
    }
}

// ─── Public Mount Entry Point ────────────────────────────────────────────────

/// Mount a FAT32 filesystem from a block device onto the VFS.
///
/// # Arguments
/// * `disk` — The block device containing the FAT32 partition.
/// * `partition_start` — Absolute sector number where the FAT32 partition begins.
/// * `mount_point` — VFS path prefix to mount files under (e.g., `"/fat"`).
///
/// # Errors
/// Returns a static error string if the boot sector is invalid, the
/// partition is not FAT32, or an I/O error occurs.
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

    Ok(())
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
