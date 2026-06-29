ENTRY(_start)

SECTIONS
{
    . = 0x100000;

    .text :
    {
        *(.text .text.*)
        *(.trampoline)
    }

    . = ALIGN(0x1000);

    .rodata ALIGN(0x1000) :
    {
        *(.rodata .rodata.*)
        *(.eh_frame_hdr)
        *(.eh_frame)
    }

    . = ALIGN(0x1000);

    .data ALIGN(0x1000) :
    {
        *(.data .data.*)
        *(.got .got.*)
    }

    . = ALIGN(0x1000);

    .bss ALIGN(0x1000) (NOLOAD) :
    {
        *(.bss .bss.*)
    }
}
