PHDRS
{
  code PT_LOAD FILEHDR PHDRS ;
  data PT_LOAD ;
}
SECTIONS
{
  . = 0x1000000 + SIZEOF_HEADERS;
  .text : {
    *(.text*)
  } :code
  . = ALIGN(0x1000);
  .rodata : {
    *(.rodata*)
  } :code
  . = ALIGN(0x1000);
  .data : {
    *(.data*)
  } :data
  .bss : {
    *(.bss*)
  } :data
}
