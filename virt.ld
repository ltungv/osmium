OUTPUT_ARCH(riscv)
ENTRY(_entry)

MEMORY {
	ram : ORIGIN = 0x80000000, LENGTH = 128M
}

PHDRS {
	/* executable sections */
	text PT_LOAD;

	/* read-only sections */
	rodata PT_LOAD;

	/* read-write sections */
	rwdata PT_LOAD;
}

SECTIONS {
	PROVIDE(_mem_size = LENGTH(ram));
	PROVIDE(_mem_addr = ORIGIN(ram));

	/* cpu instructions */
	.text : {
		*(.text.init) *(.text .text.*)
	} >ram :text

	/* cpu instructions of the trampoline */
	.tramp : {
		. = ALIGN(4096);
		PROVIDE(_tramp_addr = .);
		KEEP(*(.tramp))
	} >ram :text

	/* read-only data */
	.rodata : {
		. = ALIGN(4096);
		PROVIDE(_rodata_addr = .);
		*(.srodata .srodata.*) *(.rodata .rodata.*)
	} >ram :rodata

	ASSERT(_rodata_addr - _tramp_addr == 0x1000, "trampoline is not a single page");

	/* global variables, initialized */
	.data : {
		. = ALIGN(4096);
		PROVIDE(_data_addr = .);
		*(.sdata .sdata.*) *(.data .data.*)
	} >ram :rwdata

	/* global variables, uninitialized */
	.bss : {
		. = ALIGN(4096);
		PROVIDE(_bss_addr = .);
		*(.sbss .sbss.*) *(.bss .bss.*)
	} >ram :rwdata

	. = ALIGN(4096);
	PROVIDE(_stack_addr = .);
	PROVIDE(_heap_addr = _stack_addr + 0x10000);
}
