//! ACPI subsystem for ziqa-kernel.
//!
//! Discovers and parses ACPI tables (RSDP, RSDT/XSDT, MADT, FADT) to extract
//! platform topology: Local APIC address, I/O APIC addresses, and processor
//! count. The bootloader maps all physical memory at a fixed offset, so the
//! `AcpiHandler` impl is a trivial `phys + offset` translation with no-op
//! unmap.

use acpi::{AcpiHandler, AcpiTables, PhysicalMapping};
use core::ptr::NonNull;
use spin::Mutex;

// ── AcpiHandler implementation ───────────────────────────────────────────────

/// Zero-sized handler that translates physical addresses via the bootloader's
/// physical-memory offset.  Cloneable (required by `AcpiHandler`).
#[derive(Clone)]
pub struct KernelAcpiHandler;

impl KernelAcpiHandler {
    /// Return the physical-memory offset from the boot info.
    ///
    /// # Panics
    /// Panics if `BOOT_INFO` has not been initialised yet (must be called after
    /// `init::init()` stores boot info).
    fn phys_offset() -> u64 {
        let guard = crate::BOOT_INFO.lock();
        guard
            .as_ref()
            .expect("BOOT_INFO not initialised before ACPI init")
            .physical_memory_offset
    }
}

impl AcpiHandler for KernelAcpiHandler {
    /// Map a physical region into the kernel's virtual address space.
    ///
    /// Because the bootloader identity-maps **all** physical memory at
    /// `physical_memory_offset`, the virtual address is simply
    /// `physical_address + offset`.  No page-table manipulation is required.
    ///
    /// # Safety
    /// - `physical_address` must refer to valid physical memory (ACPI tables).
    /// - The resulting pointer is valid for at least `size` bytes.
    unsafe fn map_physical_region<T>(
        &self,
        physical_address: usize,
        size: usize,
    ) -> PhysicalMapping<Self, T> {
        let offset = Self::phys_offset();
        let virt = physical_address as u64 + offset;

        // SAFETY: The bootloader guarantees that `physical_memory_offset + phys`
        // is a valid mapping covering all of physical RAM.  ACPI tables reside
        // in reserved physical memory that the bootloader maps.
        let ptr = NonNull::new(virt as *mut T)
            .expect("ACPI: mapped virtual address was null");

        // SAFETY: `ptr` is valid, `size >= size_of::<T>()`, handler matches.
        unsafe { PhysicalMapping::new(physical_address, ptr, size, size, self.clone()) }
    }

    /// No-op: the bootloader's identity map is permanent; nothing to unmap.
    fn unmap_physical_region<T>(_region: &PhysicalMapping<Self, T>) {}
}

// ── Global ACPI state ────────────────────────────────────────────────────────

/// Summarised hardware topology extracted from the ACPI MADT.
#[derive(Debug, Clone)]
pub struct AcpiInfo {
    /// Physical address of the Local APIC (from MADT).
    pub local_apic_address: u64,
    /// Physical address of the first I/O APIC (from MADT).
    pub io_apic_address: u32,
    /// Global System Interrupt base of the first I/O APIC.
    pub io_apic_gsi_base: u32,
    /// Total number of processors (BSP + APs) discovered.
    pub processor_count: usize,
}

/// Global ACPI information, populated by [`init`].
pub static ACPI_INFO: Mutex<Option<AcpiInfo>> = Mutex::new(None);

// ── Initialisation ───────────────────────────────────────────────────────────

/// Discover ACPI tables and extract platform topology.
///
/// Scans the BIOS memory area (0x000E_0000–0x000F_FFFF) for the RSDP, then
/// walks the RSDT/XSDT to parse the MADT and FADT.  Results are stored in
/// [`ACPI_INFO`] and printed to the console.
///
/// This function is safe to call even when ACPI is not available — it prints a
/// warning and returns gracefully.
pub fn init() {
    crate::println!("[acpi] Scanning BIOS area for RSDP...");

    let handler = KernelAcpiHandler;

    // SAFETY: We are running on a BIOS-booted x86_64 system. The handler
    // correctly maps physical memory via the bootloader's identity map.
    let tables = match unsafe { AcpiTables::search_for_rsdp_bios(handler) } {
        Ok(t) => t,
        Err(e) => {
            crate::println!("[acpi] WARN: ACPI tables not found or invalid: {:?}", e);
            crate::println!("[acpi] Continuing without ACPI support.");
            return;
        }
    };

    crate::println!("[acpi] ACPI revision {}", tables.revision);
    crate::println!(
        "[acpi] Discovered {} SDT(s), DSDT {}",
        tables.sdts.len(),
        if tables.dsdt.is_some() { "present" } else { "absent" },
    );

    // List discovered table signatures.
    for sig in tables.sdts.keys() {
        crate::println!("[acpi]   table: {:?}", sig);
    }

    // ── Parse PlatformInfo (MADT + FADT) ─────────────────────────────────
    let platform_info = match tables.platform_info() {
        Ok(info) => info,
        Err(e) => {
            crate::println!("[acpi] WARN: Failed to parse platform info: {:?}", e);
            return;
        }
    };

    crate::println!("[acpi] Power profile: {:?}", platform_info.power_profile);

    // ── PM Timer ─────────────────────────────────────────────────────────
    match &platform_info.pm_timer {
        Some(pm) => {
            crate::println!(
                "[acpi] PM Timer: 32-bit={}",
                pm.supports_32bit,
            );
        }
        None => {
            crate::println!("[acpi] PM Timer: not present");
        }
    }

    // ── Interrupt model (MADT / APIC) ────────────────────────────────────
    use acpi::platform::interrupt::InterruptModel;

    match &platform_info.interrupt_model {
        InterruptModel::Apic(apic) => {
            crate::println!(
                "[acpi] Local APIC at {:#010x}",
                apic.local_apic_address,
            );
            crate::println!(
                "[acpi] {} I/O APIC(s) discovered:",
                apic.io_apics.len(),
            );
            for ioapic in &apic.io_apics {
                crate::println!(
                    "[acpi]   I/O APIC id={} addr={:#010x} GSI base={}",
                    ioapic.id,
                    ioapic.address,
                    ioapic.global_system_interrupt_base,
                );
            }

            if !apic.interrupt_source_overrides.is_empty() {
                crate::println!(
                    "[acpi] {} interrupt source override(s):",
                    apic.interrupt_source_overrides.len(),
                );
                for iso in &apic.interrupt_source_overrides {
                    crate::println!(
                        "[acpi]   ISA IRQ {} -> GSI {} (polarity={:?}, trigger={:?})",
                        iso.isa_source,
                        iso.global_system_interrupt,
                        iso.polarity,
                        iso.trigger_mode,
                    );
                }
            }

            if apic.also_has_legacy_pics {
                crate::println!("[acpi] Legacy 8259 PICs present (must be masked)");
            }

            // ── Processor topology ───────────────────────────────────────
            let proc_count = match &platform_info.processor_info {
                Some(pi) => {
                    let total = 1 + pi.application_processors.len(); // BSP + APs
                    crate::println!(
                        "[acpi] BSP: UID={} LAPIC_ID={}",
                        pi.boot_processor.processor_uid,
                        pi.boot_processor.local_apic_id,
                    );
                    for ap in &pi.application_processors {
                        crate::println!(
                            "[acpi]   AP: UID={} LAPIC_ID={} state={:?}",
                            ap.processor_uid,
                            ap.local_apic_id,
                            ap.state,
                        );
                    }
                    crate::println!("[acpi] Total processors: {}", total);
                    total
                }
                None => {
                    crate::println!("[acpi] Processor info not available in MADT");
                    1 // At least the BSP is running.
                }
            };

            // ── Store global state ───────────────────────────────────────
            let first_ioapic = apic.io_apics.first();
            let info = AcpiInfo {
                local_apic_address: apic.local_apic_address,
                io_apic_address: first_ioapic.map_or(0, |io| io.address),
                io_apic_gsi_base: first_ioapic.map_or(0, |io| io.global_system_interrupt_base),
                processor_count: proc_count,
            };
            *ACPI_INFO.lock() = Some(info);
        }

        InterruptModel::Unknown => {
            crate::println!("[acpi] Interrupt model: Unknown (legacy 8259 PIC only?)");
            // No APIC info to store.
        }

        // The enum is non_exhaustive; handle future variants gracefully.
        _ => {
            crate::println!("[acpi] Interrupt model: unrecognised variant");
        }
    }

    crate::println!("[acpi] Initialisation complete.");
}
