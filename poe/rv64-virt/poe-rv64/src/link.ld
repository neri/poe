ENTRY(_start)

SECTIONS
{
    /* Position independent: linked at 0, relocated by _start at runtime */
    . = 0;
    _image_start = .;
    .text : { KEEP(*(.text.head)) *(.text .text.*) }
    .rodata : { *(.rodata .rodata.* .srodata .srodata.*) }
    .dynsym : { *(.dynsym) }
    .dynstr : { *(.dynstr) }
    .hash : { *(.hash) }
    .gnu.hash : { *(.gnu.hash) }
    .rela.dyn : {
        __rela_start = .;
        *(.rela.dyn .rela.*)
        __rela_end = .;
    }
    . = ALIGN(8);
    .dynamic : { *(.dynamic) }
    .got : { *(.got .got.*) }
    .data : { *(.data .data.* .sdata .sdata.*) }
    .bss (NOLOAD) : {
        . = ALIGN(16);
        __bss_start = .;
        *(.bss .bss.* .sbss .sbss.*)
        *(COMMON)
        . = ALIGN(8);
        __bss_end = .;
        . = ALIGN(4096);
        . += 128 * 1024;
        __stack_top = .;
    }
    _end = .;

   /DISCARD/ : { *(.comment) *(.gnu*) *(.note*) *(.eh_frame*) *(.interp) }
}
__image_size = _end - _image_start;
