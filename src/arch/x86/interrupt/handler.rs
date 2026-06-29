/// x86 register state and interrupt stack definitions
/// Ported from Redox OS.

use crate::println;

#[derive(Default, Debug, Clone, Copy)]
#[repr(C, packed)]
pub struct ScratchRegisters {
    pub eax: usize,
    pub ecx: usize,
    pub edx: usize,
}

#[derive(Default, Debug, Clone, Copy)]
#[repr(C, packed)]
pub struct PreservedRegisters {
    pub ebp: usize,
    pub esi: usize,
    pub edi: usize,
    pub ebx: usize,
}

#[derive(Default, Debug, Clone, Copy)]
#[repr(C, packed)]
pub struct IretRegisters {
    pub eip: usize,
    pub cs: usize,
    pub eflags: usize,
    pub esp: usize,
    pub ss: usize,
}

#[derive(Default, Debug, Clone, Copy)]
#[repr(C, packed)]
pub struct InterruptStack {
    pub gs: usize,
    pub preserved: PreservedRegisters,
    pub scratch: ScratchRegisters,
    pub iret: IretRegisters,
}

impl InterruptStack {
    pub fn dump(&self) {
        println!("EIP:   0x{:08x}", self.iret.eip);
        println!("CS:    0x{:04x}", self.iret.cs);
        println!("EFLAG: 0x{:08x}", self.iret.eflags);
        println!("EAX:   0x{:08x}", self.scratch.eax);
        println!("ECX:   0x{:08x}", self.scratch.ecx);
        println!("EDX:   0x{:08x}", self.scratch.edx);
        println!("EBX:   0x{:08x}", self.preserved.ebx);
        println!("ESI:   0x{:08x}", self.preserved.esi);
        println!("EDI:   0x{:08x}", self.preserved.edi);
        println!("EBP:   0x{:08x}", self.preserved.ebp);
    }

    pub fn init(&mut self) {
        self.iret.eflags = 0x200; // IF
    }
}

#[macro_export]
macro_rules! push_scratch {
    () => { "push ecx\npush edx\n" };
}

#[macro_export]
macro_rules! pop_scratch {
    () => { "pop edx\npop ecx\npop eax\n" };
}

#[macro_export]
macro_rules! push_preserved {
    () => { "push ebx\npush edi\npush esi\npush ebp\n" };
}

#[macro_export]
macro_rules! pop_preserved {
    () => { "pop ebp\npop esi\npop edi\npop ebx\n" };
}

#[macro_export]
macro_rules! enter_kernel_gs {
    () => {
        "push ecx\nmov ecx, gs\npush ecx\nmov ecx, 0x18\nmov gs, ecx\n"
    };
}

#[macro_export]
macro_rules! exit_kernel_gs {
    () => { "pop ecx\nmov gs, ecx\npop ecx\n" };
}

#[macro_export]
macro_rules! interrupt_stack {
    ($name:ident, |$stack:ident| $code:block) => {
        #[unsafe(naked)]
        pub unsafe extern "C" fn $name() {
            unsafe extern "fastcall" fn inner($stack: &mut InterruptStack) {
                #[allow(unused_unsafe)]
                unsafe { $code }
            }
            core::arch::naked_asm!(
                "push eax",
                push_scratch!(),
                push_preserved!(),
                enter_kernel_gs!(),
                "mov ecx, esp",
                "call {inner}",
                exit_kernel_gs!(),
                pop_preserved!(),
                pop_scratch!(),
                "iretd",
                inner = sym inner,
            );
        }
    };
}
