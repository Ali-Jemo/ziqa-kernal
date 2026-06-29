//! ACPI scheme for ZiqaKernel.
//! Exposes ACPI topology and raw SDT access as a filesystem-like namespace:
//!   acpi:                  - scheme root
//!   acpi/rsdp              - RSDP physical address (8 bytes LE)
//!   acpi/rsdt              - list of SDT physical addresses (compact binary)
//!   acpi/tables            - discovered ACPI table signatures + phys addrs
//!   acpi/info              - summarised MADT/FADT topology
//!   acpi/controller/<sig>  - raw bytes of a specific table (by 4-char signature)

use crate::scheme::{Scheme, SchemeResult};
use crate::abi::AbiError;
use alloc::collections::BTreeMap;
use alloc::string::String;
use core::slice;
use core::sync::atomic::{AtomicUsize, Ordering};
use spin::Mutex;

// ── Helpers ─────────────────────────────────────────────────────────────────

fn phys_offset() -> u64 {
    crate::BOOT_INFO
        .lock()
        .as_ref()
        .expect("BOOT_INFO not initialised before ACPI scheme use")
        .physical_memory_offset
}

fn acpi_info() -> Option<crate::drivers::acpi::AcpiInfo> {
    let guard = crate::drivers::acpi::ACPI_INFO.lock();
    guard.clone()
}

fn table_by_sig(sig: &[u8; 4]) -> Option<crate::drivers::acpi::AcpiTableInfo> {
    let info = crate::drivers::acpi::ACPI_INFO.lock();
    info.as_ref()?.tables.iter().find(|t| &t.signature == sig).cloned()
}

// ── Handle mapping ──────────────────────────────────────────────────────────

#[derive(Clone, Copy)]
enum AcpiTarget {
    Root,
    Rsdp,
    Rsdt,
    Tables,
    Info,
    Table { _sig: [u8; 4], phys: usize, len: usize },
}

// ── Scheme ───────────────────────────────────────────────────────────────────

pub struct AcpiScheme {
    handles: Mutex<BTreeMap<usize, AcpiTarget>>,
    next_handle: AtomicUsize,
}

impl AcpiScheme {
    pub const fn new() -> Self {
        Self {
            handles: Mutex::new(BTreeMap::new()),
            next_handle: AtomicUsize::new(1),
        }
    }

    fn alloc_handle(&self, target: AcpiTarget) -> usize {
        let id = self.next_handle.fetch_add(1, Ordering::Relaxed);
        self.handles.lock().insert(id, target);
        id
    }
}

impl Scheme for AcpiScheme {
    fn open(&self, path: &str, _flags: usize) -> SchemeResult<usize> {
        let target = match path {
            "acpi" => AcpiTarget::Root,
            "acpi/rsdp" => AcpiTarget::Rsdp,
            "acpi/rsdt" => AcpiTarget::Rsdt,
            "acpi/tables" => AcpiTarget::Tables,
            "acpi/info" => AcpiTarget::Info,
            s if s.starts_with("acpi/controller/") => {
                let sig = &s["acpi/controller/".len()..];
                if sig.len() != 4 {
                    return Err(AbiError::Other("ACPI table signature must be 4 chars"));
                }
                let mut bytes = [0u8; 4];
                bytes.copy_from_slice(sig.as_bytes());
                let Some(t) = table_by_sig(&bytes) else {
                    return Err(AbiError::Other("ACPI table not found"));
                };
                AcpiTarget::Table {
                    _sig: bytes,
                    phys: t.physical_address,
                    len: t.length,
                }
            }
            _ => return Err(AbiError::Other("unknown acpi path")),
        };
        Ok(self.alloc_handle(target))
    }

    fn read(&self, id: usize, buf: &mut [u8]) -> SchemeResult<usize> {
        let target = *self.handles.lock().get(&id).ok_or(AbiError::Other("bad handle"))?;
        match target {
            AcpiTarget::Root => {
                let s = b"acpi:\n";
                let n = buf.len().min(s.len());
                buf[..n].copy_from_slice(&s[..n]);
                Ok(n)
            }
            AcpiTarget::Rsdp => {
                let Some(info) = acpi_info() else {
                    return Err(AbiError::Other("ACPI tables not available"));
                };
                let n = buf.len().min(8);
                buf[..n].copy_from_slice(&(info.rsdp_address as u64).to_le_bytes()[..n]);
                Ok(n)
            }
            AcpiTarget::Rsdt => {
                let Some(info) = acpi_info() else {
                    return Err(AbiError::Other("ACPI tables not available"));
                };
                let entries = info.tables.len();
                let total = 8 + 4 + (entries.min(12) * 4);
                let out = buf.len().min(total);
                let mut written = 0usize;

                buf[..8].copy_from_slice(&(info.rsdp_address as u64).to_le_bytes());
                written += 8;

                if out >= written + 4 {
                    buf[written..written + 4].copy_from_slice(&(entries as u32).to_le_bytes());
                    written += 4;
                }

                let slot = &mut buf[written..out];
                for (i, t) in info.tables.iter().take(12).enumerate() {
                    if (i + 1) * 4 > slot.len() {
                        break;
                    }
                    slot[i * 4..i * 4 + 4]
                        .copy_from_slice(&(t.physical_address as u32).to_le_bytes());
                }
                Ok(out)
            }
            AcpiTarget::Tables => {
                let Some(info) = acpi_info() else {
                    return Err(AbiError::Other("ACPI tables not available"));
                };
                let mut out = String::new();
                for t in &info.tables {
                    use alloc::fmt::Write;
                    let sig = core::str::from_utf8(&t.signature).unwrap_or("????");
                    let _ = writeln!(out, "{sig} 0x{:x} {}\n", t.physical_address, t.length);
                }
                let bytes = out.as_bytes();
                let n = buf.len().min(bytes.len());
                buf[..n].copy_from_slice(&bytes[..n]);
                Ok(n)
            }
            AcpiTarget::Info => {
                let Some(info) = acpi_info() else {
                    return Err(AbiError::Other("ACPI platform info not available"));
                };
                let text = alloc::format!(
                    "local_apic=0x{:x}\nio_apic=0x{:x}\nio_apic_gsi_base={}\nprocessors={}\n",
                    info.local_apic_address,
                    info.io_apic_address,
                    info.io_apic_gsi_base,
                    info.processor_count,
                );
                let bytes = text.as_bytes();
                let n = buf.len().min(bytes.len());
                buf[..n].copy_from_slice(&bytes[..n]);
                Ok(n)
            }
            AcpiTarget::Table { phys, len, _sig: _ } => {
                let n = buf.len().min(len);
                if n == 0 {
                    return Err(AbiError::Other("ACPI table empty"));
                }
                // SAFETY: tables live in physical memory during boot;
                // drivers/acpi validated them and they're never freed.
                unsafe {
                    let po = phys_offset();
                    let src = (phys as u64 + po) as *const u8;
                    buf[..n].copy_from_slice(slice::from_raw_parts(src, n));
                }
                Ok(n)
            }
        }
    }

    fn write(&self, _id: usize, _buf: &[u8]) -> SchemeResult<usize> {
        Err(AbiError::Other("ACPI tables are read-only"))
    }

    fn close(&self, id: usize) -> SchemeResult<()> {
        self.handles.lock().remove(&id);
        Ok(())
    }
}
