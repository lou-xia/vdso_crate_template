OUTPUT_ARCH({output_arch})

SECTIONS {
    . = SIZEOF_HEADERS;

    /* 先放置动态链接相关的只读段 */
    .hash		: { *(.hash) }
    .gnu.hash	: { *(.gnu.hash) }
    .dynsym		: { *(.dynsym) }
    .dynstr		: { *(.dynstr) }
    .gnu.version	: { *(.gnu.version) }
    .gnu.version_d	: { *(.gnu.version_d) }
    .gnu.version_r	: { *(.gnu.version_r) }

    /* 动态段单独分配 */
    .dynamic    : { *(.dynamic) }

    . = ALIGN(16);
    /* 代码段（.text）需要放在只读数据段之前 */
    .text       : {
        *(.text.start)
        *(.text .text.*)
    }

    . = ALIGN(4K);
    /* 只读数据段（.rodata等） */
    .rodata     : {
        *(.rodata .rodata.* .gnu.linkonce.r.*)
        *(.note.*)
    }

    . = ALIGN(4K);
    .plt : { *(.plt .plt.*) }

    . = ALIGN(4K);
    /* 数据段（.data、.bss等）单独分配 */
    .data       : {
        *(.data .data.* .gnu.linkonce.d.*)
        *(.got.plt) *(.got)
    }

    . = ALIGN(4K);
    .bss        : {
        *(.bss .bss.* .gnu.linkonce.b.*)
        *(COMMON)
    }

    .eh_frame_hdr	: { *(.eh_frame_hdr) }
    .eh_frame	: { KEEP (*(.eh_frame)) }
}