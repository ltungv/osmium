OUTPUT_ARCH(riscv)

/*
We're setting our entry point to a symbol called _start which is inside of boot.S.
This essentially stores the address of _start as the "entry point", or where CPU
instructions should start executing.
*/
ENTRY(_start)

/*
The MEMORY section will explain that we have "ram" that contains a section that is
'w' (writeable), 'x' (executable), and 'a' (allocatable).  We use '!' to invert
'r' (read-only) and 'i' (initialized). We don't want our memory to be read-only,
and we're stating that it is NOT initialized at the beginning.

LENGTH = 128M tells the linker that we have 128 megabyte of RAM. The linker will
double check this to make sure everything can fit.
*/
MEMORY {
	ram (wxa!ri) : ORIGIN = 0x80000000, LENGTH = 128M
}

/*
PHDRS is short for "program headers"
text - CPU instructions (executable sections)
data - Global, initialized variables
bss  - Global, uninitialized variables (all will be set to 0 by boot.S)

PT_LOAD tells the linker that these sections will be loaded from the file into memory.
*/
PHDRS {
	text PT_LOAD;
	data PT_LOAD;
	bss PT_LOAD;
}

SECTIONS {
	.text : {
		PROVIDE(_text_start = .);
		*(.text.init) *(.text .text.*)
		PROVIDE(_text_end = .);
	} >ram AT>ram :text

	PROVIDE(_global_pointer = .);

	.rodata : {
		PROVIDE(_rodata_start = .);
		*(.rodata .rodata.*)
		PROVIDE(_rodata_end = .);
	} >ram AT>ram :text

	.data : {
		. = ALIGN(4096);
		PROVIDE(_data_start = .);
		*(.sdata .sdata.*) *(.data .data.*)
		PROVIDE(_data_end = .);
	} >ram AT>ram :data

	.bss : {
		PROVIDE(_bss_start = .);
		*(.sbss .sbss.*) *(.bss .bss.*)
		PROVIDE(_bss_end = .);
	} >ram AT>ram :bss

	PROVIDE(_memory_start = ORIGIN(ram));
	PROVIDE(_memory_end = ORIGIN(ram) + LENGTH(ram));

	PROVIDE(_stack_size = 0x80000);
	PROVIDE(_stack_start = _bss_end);
	PROVIDE(_stack_end = _stack_start + _stack_size);

	PROVIDE(_heap_size = _memory_end - _stack_end);
	PROVIDE(_heap_start = _stack_end);
	PROVIDE(_heap_start = _heap_start + _heap_size);
}