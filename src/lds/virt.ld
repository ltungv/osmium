/* 
 * linker script for outputting to risc-v qemu "virt" machine.
 *
 */

/* 
 * set risc-v as the architecture.
 *
 */
OUTPUT_ARCH(riscv)

/* 
 * set the entry point to the `_start` symbol located inside `boot.s`.
 *
 */
ENTRY(_start)


/* 
 * the memory section tells that there's a "ram" containing a section that is:
 * - 'w': writeable
 * - 'x': executable
 * - 'a': allocatable
 * - '!r': not read-only
 * - '!i': not initialized
 *
 * there's 128 megabytes of ram available.
 *
 */
MEMORY {
	ram (wxa!ri) : ORIGIN = 0x80000000, LENGTH = 128M
}

/* phdrs is short for "program headers":
 * - text: cpu instructions (executable sections)
 * - data: global, initialized variables
 * - bss: global, uninitialized variables (all will be set to 0 by boot.s)
 *
 * pt_load tells the linker that theses sections will be loaded from file into memory.
 *
 */
PHDRS {
	text PT_LOAD;
	data PT_LOAD;
	bss PT_LOAD;
}

/* the memory is organized based on which section it is in. in rust, we have:
 * - cpu instructions stored in `text`.
 * - global constants stored in `rodata`.
 * - global initialized variables stored in `data`.
 * - global uninitialized variables stored in `bss`.
 */
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

	PROVIDE(_stack_start = _bss_end + 0x80000);
	PROVIDE(_stack_end = _stack_start + 0x80000);

	PROVIDE(_heap_start = _stack_end);
	PROVIDE(_heap_size = _memory_end - _heap_start);
}

