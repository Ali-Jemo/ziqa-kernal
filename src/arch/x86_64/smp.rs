use crate::arch::x86_64::apic;
use crate::arch::x86_64::per_cpu;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use x86_64::instructions::interrupts;
use x86_64::registers::control::Cr3;
use x86_64::structures::paging::mapper::MapToError;
use x86_64::structures::paging::{FrameAllocator, Mapper, Page, PageTableFlags, PhysFrame, Size4KiB};
use x86_64::PhysAddr;
use x86_64::VirtAddr;

pub const TRAMPOLINE_PHYS: u64 = 0x7000;
pub const TRAMPOLINE_PAGE: u64 = 0x7000;

/// Boot info block placed at a fixed offset within the trampoline page.
/// Offsets within trampoline page:
const BOOT_INFO_OFFSET: u64 = 0x1E0;
const GDT_OFFSET: u64 = 0x100;
const GDT_PTR_OFFSET: u64 = 0x140;

extern "C" {
    static trampoline_start: u8;
    static trampoline_end: u8;
}

core::arch::global_asm!(
    r#"
.pushsection .trampoline, "ax"
.code16
trampoline_start:
    cli
    xor ax, ax
    mov ds, ax
    mov es, ax
    mov ss, ax
    xor sp, sp
    mov sp, 0x7000

    in al, 0x92
    or al, 2
    out 0x92, al

    mov si, 0x7140
    lgdt cs:[si]

    mov eax, cr0
    or al, 1
    mov cr0, eax

    .byte 0x66, 0xEA
    .long (protected_mode_start - trampoline_start + 0x7000)
    .word 0x08

.code32
protected_mode_start:
    mov ax, 0x10
    mov ds, ax
    mov es, ax
    mov ss, ax

    mov eax, cr4
    or eax, 0x20
    mov cr4, eax

    mov eax, [0x71E0]
    mov cr3, eax

    mov ecx, 0xC0000080
    rdmsr
    or eax, 0x100
    wrmsr

    mov eax, cr0
    or eax, 0x80000000
    mov cr0, eax

    .byte 0xEA
    .long (long_mode_start - trampoline_start + 0x7000)
    .word 0x18

.code64
long_mode_start:
    mov rax, 0x7140
    lgdt [rax]

    mov ax, 0x10
    mov ds, ax
    mov es, ax
    mov ss, ax

    mov rax, [0x71E8]
    mov rcx, 0xC0000101
    mov rdx, rax
    shr rdx, 32
    wrmsr

    mov rax, [0x71F0]
    mov rsp, rax

    mov rax, [0x71F8]
    jmp rax

trampoline_end:
.popsection
"#,
);

static APS_READY: AtomicU32 = AtomicU32::new(0);
static _AP_BOOT_ERROR: AtomicBool = AtomicBool::new(false);

pub fn boot_aps(acpi_info: &crate::drivers::acpi::AcpiInfo) {
    if acpi_info.processor_count <= 1 {
        crate::klog!(crate::klog::Level::Info, "SMP: only 1 processor, skipping AP boot");
        return;
    }

    let apic_ids: Vec<u32> = get_ap_apic_ids();
    if apic_ids.is_empty() {
        crate::klog!(crate::klog::Level::Warn, "SMP: no APs found in MADT");
        return;
    }

    crate::klog!(
        crate::klog::Level::Info,
        "SMP: booting {} AP(s)",
        apic_ids.len(),
    );

    setup_trampoline().expect("SMP: failed to setup trampoline");

    let cr3_val = Cr3::read().0.start_address().as_u64();

    for (cpu_index, &apic_id) in apic_ids.iter().enumerate() {
        let cpu_id = (cpu_index + 1) as u32;
        let per_cpu_ptr = per_cpu::alloc_ap(apic_id, cpu_id);

        let stack = allocate_ap_stack();
        per_cpu_ptr.kernel_stack_top = stack;
        per_cpu_ptr.kernel_stack_ptr = stack;
        per_cpu_ptr.cr3 = cr3_val;

        fill_boot_info(cr3_val, per_cpu_ptr as *const _ as u64, stack, cpu_id);

        start_ap(apic_id, (TRAMPOLINE_PAGE >> 12) as u8, cpu_id);
    }

    let total_aps = apic_ids.len() as u32;
    for _ in 0..1000000 {
        if APS_READY.load(Ordering::Acquire) >= total_aps {
            break;
        }
        core::hint::spin_loop();
    }

    let started = APS_READY.load(Ordering::Acquire);
    crate::klog!(
        crate::klog::Level::Info,
        "SMP: {}/{} APs started",
        started,
        total_aps,
    );

    if started > 0 {
        crate::klog!(
            crate::klog::Level::Info,
            "SMP: system now has {} CPU(s) online",
            per_cpu::cpu_count(),
        );
    }
}

fn get_ap_apic_ids() -> Vec<u32> {
    let apic_ids = Vec::new();
    let handler = crate::drivers::acpi::KernelAcpiHandler;
    if let Ok(tables) = unsafe { acpi::AcpiTables::search_for_rsdp_bios(handler) } {
        if let Ok(platform_info) = tables.platform_info() {
            use acpi::platform::interrupt::InterruptModel;
            if let InterruptModel::Apic(_apic) = &platform_info.interrupt_model {
                if let Some(pi) = &platform_info.processor_info {
                    let mut ids = Vec::new();
                    for ap in &pi.application_processors {
                        ids.push(ap.local_apic_id);
                        crate::klog!(
                            crate::klog::Level::Info,
                            "SMP: discovered AP: UID={} LAPIC_ID={}",
                            ap.processor_uid,
                            ap.local_apic_id,
                        );
                    }
                    return ids;
                }
            }
        }
    }
    apic_ids
}

fn setup_trampoline() -> Result<(), &'static str> {
    let phys_offset = get_phys_offset();
    let tramp_virt = phys_offset + TRAMPOLINE_PHYS;

    let trampoline_size = unsafe {
        (&trampoline_end as *const u8 as usize) - (&trampoline_start as *const u8 as usize)
    };

    if trampoline_size > 0x200 {
        return Err("trampoline too large");
    }

    unsafe {
        core::ptr::write_bytes(tramp_virt as *mut u8, 0, 0x200);
        core::ptr::copy_nonoverlapping(
            &trampoline_start as *const u8,
            tramp_virt as *mut u8,
            trampoline_size,
        );
    }

    identity_map_page(TRAMPOLINE_PHYS)?;

    let gdt_data = build_trampoline_gdt();
    unsafe {
        let gdt_dst = (tramp_virt + GDT_OFFSET) as *mut u8;
        core::ptr::copy_nonoverlapping(gdt_data.as_ptr(), gdt_dst, gdt_data.len());

        let gdt_base = TRAMPOLINE_PHYS + GDT_OFFSET;
        let limit = (gdt_data.len() - 1) as u16;
        let gdt_ptr_dst = (tramp_virt + GDT_PTR_OFFSET) as *mut u8;
        let ptr_bytes: [u8; 10] = [
            limit as u8,
            (limit >> 8) as u8,
            gdt_base as u8,
            (gdt_base >> 8) as u8,
            (gdt_base >> 16) as u8,
            (gdt_base >> 24) as u8,
            (gdt_base >> 32) as u8,
            (gdt_base >> 40) as u8,
            (gdt_base >> 48) as u8,
            (gdt_base >> 56) as u8,
        ];
        core::ptr::copy_nonoverlapping(ptr_bytes.as_ptr(), gdt_ptr_dst, 10);
    }

    crate::klog!(
        crate::klog::Level::Info,
        "SMP: trampoline {}-byte at {:#x} (virt {:#x})",
        trampoline_size,
        TRAMPOLINE_PHYS,
        tramp_virt,
    );

    Ok(())
}

fn build_trampoline_gdt() -> [u8; 32] {
    let mut gdt = [0u8; 32];
    // Null descriptor at offset 0
    // Code32 at offset 8: base=0, limit=0xFFFFF, access=0x9B, flags=0xCF
    gdt[8..16].copy_from_slice(&[0xFF, 0xFF, 0x00, 0x00, 0x00, 0x9B, 0xCF, 0x00]);
    // Data at offset 16: base=0, limit=0xFFFFF, access=0x93, flags=0xCF
    gdt[16..24].copy_from_slice(&[0xFF, 0xFF, 0x00, 0x00, 0x00, 0x93, 0xCF, 0x00]);
    // Code64 at offset 24: base=0, limit=0xFFFFF, access=0x9B, flags=0xAF
    gdt[24..32].copy_from_slice(&[0xFF, 0xFF, 0x00, 0x00, 0x00, 0x9B, 0xAF, 0x00]);
    gdt
}

fn fill_boot_info(cr3: u64, per_cpu_ptr: u64, stack_top: u64, _cpu_id: u32) {
    let phys_offset = get_phys_offset();
    let bi_virt = phys_offset + TRAMPOLINE_PHYS + BOOT_INFO_OFFSET;
    unsafe {
        let bi = bi_virt as *mut u64;
        *bi = cr3;
        *bi.add(1) = per_cpu_ptr;
        *bi.add(2) = stack_top;
        *bi.add(3) = ap_entry as *const () as u64;
    }
}

fn identity_map_page(phys: u64) -> Result<(), &'static str> {
    let page: Page<Size4KiB> = Page::containing_address(VirtAddr::new(phys));
    let frame: PhysFrame<Size4KiB> = PhysFrame::containing_address(PhysAddr::new(phys));
    let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;

    let mut mapper_guard = crate::memory::paging::KERNEL_MAPPER.lock();
    let mapper = mapper_guard.as_mut().ok_or("KERNEL_MAPPER not initialized")?;
    let mut fa_guard = crate::memory::FRAME_ALLOCATOR.lock();
    let fa = fa_guard.as_mut().ok_or("FRAME_ALLOCATOR not initialized")?;

    match unsafe { Mapper::map_to(&mut mapper.mapper, page, frame, flags, fa) } {
        Ok(flusher) => {
            flusher.flush();
            Ok(())
        }
        Err(MapToError::PageAlreadyMapped(_)) => Ok(()),
        Err(e) => {
            crate::klog!(crate::klog::Level::Warn, "SMP: identity map failed: {:?}", e);
            Err("identity map failed")
        }
    }
}

fn start_ap(apic_id: u32, vector: u8, cpu_idx: u32) {
    apic::send_init_ipi(apic_id);
    let mut delay = 10_000;
    while delay > 0 {
        delay -= 1;
        core::hint::spin_loop();
    }

    apic::send_sipi(apic_id, vector);
    delay = 200;
    while delay > 0 {
        delay -= 1;
        core::hint::spin_loop();
    }

    apic::send_sipi(apic_id, vector);
    crate::klog!(
        crate::klog::Level::Info,
        "SMP: INIT-SIPI-SIPI sent to APIC {} (vector {:#x}), cpu_idx {}",
        apic_id, vector, cpu_idx,
    );
}

fn allocate_ap_stack() -> u64 {
    const STACK_SIZE: usize = 16384;
    let mut fa_guard = crate::memory::FRAME_ALLOCATOR.lock();
    let fa = fa_guard.as_mut().expect("FRAME_ALLOCATOR not initialized");
    let frame = fa.allocate_frame().expect("out of memory for AP stack");
    drop(fa_guard);

    let phys = frame.start_address().as_u64();
    let po = get_phys_offset();
    unsafe {
        core::ptr::write_bytes((po + phys) as *mut u8, 0, STACK_SIZE);
    }
    phys + STACK_SIZE as u64
}

fn get_phys_offset() -> u64 {
    let guard = crate::BOOT_INFO.lock();
    guard
        .as_ref()
        .expect("BOOT_INFO not initialized")
        .physical_memory_offset
}

/// Called by the AP trampoline after entering long mode.
/// This is the first Rust code the AP executes.
#[no_mangle]
pub extern "C" fn ap_entry() -> ! {
    let cpu = per_cpu::current_cpu();

    interrupts::enable();

    let cpu_id = cpu.cpu_id;
    crate::klog!(
        crate::klog::Level::Info,
        "SMP: AP {} (APIC ID {}) online",
        cpu_id,
        cpu.apic_id,
    );

    crate::arch::x86_64::gdt::init_ap();

    apic::enable();
    let _ = apic::calibrate_timer(5);
    apic::start_periodic_timer(apic::read_lapic(apic::LAPIC_TIMER_CURCNT));

    APS_READY.fetch_add(1, Ordering::Release);

    ap_idle_loop()
}

fn ap_idle_loop() -> ! {
    loop {
        interrupts::disable();
        if per_cpu::current_cpu().current_pid().is_some() {
            interrupts::enable();
            crate::process::scheduler::SCHEDULER.schedule();
            continue;
        }
        interrupts::enable();
        x86_64::instructions::hlt();
    }
}

pub fn send_reschedule_ipi(cpu_id: u32) {
    if let Some(cpu) = per_cpu::per_cpu_by_id(cpu_id) {
        apic::send_ipi_fixed(cpu.apic_id, apic::IPI_RESCHEDULE_VECTOR);
    }
}

pub fn send_reschedule_ipi_to(cpu: &per_cpu::PerCpu) {
    apic::send_ipi_fixed(cpu.apic_id, apic::IPI_RESCHEDULE_VECTOR);
}

pub fn send_reschedule_ipi_all() {
    apic::send_ipi_broadcast(
        apic::IPI_RESCHEDULE_VECTOR,
        apic::IcrDestShorthand::AllExcludingSelf,
    );
}

pub fn tlb_shootdown_all(addr: u64) {
    let cpu_count = per_cpu::cpu_count();
    let this_id = per_cpu::current_cpu().cpu_id;

    for id in 0..cpu_count {
        if id != this_id {
            if let Some(cpu) = per_cpu::per_cpu_by_id_mut(id) {
                cpu.tlb_shootdown_in_progress = true;
                cpu.tlb_shootdown_addr = addr;
                apic::send_ipi_fixed(cpu.apic_id, apic::IPI_TLB_SHOOTDOWN_VECTOR);
            }
        }
    }

    for id in 0..cpu_count {
        if id != this_id {
            if let Some(cpu) = per_cpu::per_cpu_by_id(id) {
                while cpu.tlb_shootdown_in_progress {
                    core::hint::spin_loop();
                }
            }
        }
    }
}

pub fn tlb_shootdown_page(addr: u64) {
    tlb_shootdown_all(addr);
}

pub fn handle_tlb_shootdown() {
    let addr = per_cpu::current_cpu().tlb_shootdown_addr;
    if addr == 0 {
        x86_64::instructions::tlb::flush_all();
    } else {
        x86_64::instructions::tlb::flush(VirtAddr::new(addr));
    }
    per_cpu::current_cpu_mut().tlb_shootdown_in_progress = false;
}

pub fn handle_reschedule_ipi() {
    let cpu = per_cpu::current_cpu();
    if cpu.current_pid().is_some() {
        crate::process::scheduler::SCHEDULER.schedule();
    }
}
