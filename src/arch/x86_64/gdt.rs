use lazy_static::lazy_static;
use x86_64::structures::gdt::{GlobalDescriptorTable, SegmentSelector};
use x86_64::structures::tss::TaskStateSegment;
use x86_64::VirtAddr;

pub const DOUBLE_FAULT_IST_INDEX: u16 = 0;

lazy_static! {
    static ref TSS: TaskStateSegment = {
        let mut tss = TaskStateSegment::new();
        tss.interrupt_stack_table[DOUBLE_FAULT_IST_INDEX as usize] = {
            const STACK_SIZE: usize = 4096 * 5;
            static mut STACK: [u8; STACK_SIZE] = [0; STACK_SIZE];
            let stack_start = core::ptr::addr_of!(STACK) as u64;
            VirtAddr::new(stack_start + STACK_SIZE as u64)
        };
        tss
    };
}

lazy_static! {
    static ref GDT: (GlobalDescriptorTable, Selectors) = {
        let mut gdt = GlobalDescriptorTable::new();
        let code_selector =
            gdt.add_entry(x86_64::structures::gdt::Descriptor::kernel_code_segment());

        // Kernel Data Segment: required for STAR. STAR expects kernel SS = kernel CS + 8.
        let kernel_data_flags = x86_64::structures::gdt::DescriptorFlags::USER_SEGMENT
            | x86_64::structures::gdt::DescriptorFlags::PRESENT
            | x86_64::structures::gdt::DescriptorFlags::WRITABLE;
        let kernel_data_selector = gdt.add_entry(x86_64::structures::gdt::Descriptor::UserSegment(kernel_data_flags.bits()));

        let user_data_selector =
            gdt.add_entry(x86_64::structures::gdt::Descriptor::user_data_segment());
        let user_code_selector =
            gdt.add_entry(x86_64::structures::gdt::Descriptor::user_code_segment());
        let tss_selector = gdt.add_entry(x86_64::structures::gdt::Descriptor::tss_segment(&TSS));
        (
            gdt,
            Selectors {
                code_selector,
                kernel_data_selector,
                user_code_selector,
                user_data_selector,
                tss_selector,
            },
        )
    };
}

struct Selectors {
    code_selector: SegmentSelector,
    kernel_data_selector: SegmentSelector,
    user_code_selector: SegmentSelector,
    user_data_selector: SegmentSelector,
    tss_selector: SegmentSelector,
}

pub fn user_code_selector() -> SegmentSelector {
    GDT.1.user_code_selector
}
pub fn user_data_selector() -> SegmentSelector {
    GDT.1.user_data_selector
}
pub fn kernel_data_selector() -> SegmentSelector {
    GDT.1.kernel_data_selector
}

pub fn init() {
    use x86_64::instructions::segmentation::{Segment, CS, DS, ES, SS};
    use x86_64::instructions::tables::load_tss;

    GDT.0.load();
    unsafe {
        CS::set_reg(GDT.1.code_selector);
        DS::set_reg(GDT.1.kernel_data_selector);
        ES::set_reg(GDT.1.kernel_data_selector);
        SS::set_reg(GDT.1.kernel_data_selector);
        load_tss(GDT.1.tss_selector);
    }
}

pub fn set_tss_stack(stack: VirtAddr) {
    unsafe {
        let tss_ptr = &*TSS as *const TaskStateSegment as *mut TaskStateSegment;
        (*tss_ptr).privilege_stack_table[0] = stack;
    }
}

pub fn init_ap() {
    use x86_64::instructions::segmentation::{Segment, CS, DS, ES, SS};
    use x86_64::instructions::tables::load_tss;

    GDT.0.load();
    unsafe {
        CS::set_reg(GDT.1.code_selector);
        DS::set_reg(GDT.1.kernel_data_selector);
        ES::set_reg(GDT.1.kernel_data_selector);
        SS::set_reg(GDT.1.kernel_data_selector);
        load_tss(GDT.1.tss_selector);
    }
}
