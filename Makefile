################
# BUILD
################
CC=riscv64-unknown-elf-gcc
CFLAGS=-std=c++17 -Wall -Wextra -pedantic -O0 -g
CFLAGS+=-static -nostdlib -ffreestanding -fno-rtti -fno-exceptions
CFLAGS+=-march=rv64gc -mabi=lp64d
LINKER_SCRIPT=-Tsrc/lds/virt.lds
TYPE=debug
RUST_TARGET=./target/riscv64gc-unknown-none-elf/$(TYPE)
LIBS=-L$(RUST_TARGET)
SOURCES_ASM=$(wildcard src/asm/*.S)
LIB=-losmium -lgcc
OUT=os.elf

################
# QEMU
################
QEMU=qemu-system-riscv64
MACH=virt
CPU=rv64
CPUS=4
MEM=128M
DRIVE=hdd.dsk

all:
	cargo build
	$(CC) $(CFLAGS) $(LINKER_SCRIPT) $(SOURCES_ASM) $(LIBS) $(LIB) -o $(OUT)

run: all
	$(QEMU) \
	-machine $(MACH) \
	-m $(MEM) \
	-cpu $(CPU) \
	-smp $(CPUS) \
	-nographic \
	-serial mon:stdio \
	-bios none \
	-kernel $(OUT) \
	-drive if=none,format=raw,file=$(DRIVE),id=primary \
	-device virtio-blk-device,drive=primary

dbg: all
	$(QEMU) \
	-machine $(MACH) \
	-m $(MEM) \
	-cpu $(CPU) \
	-smp $(CPUS) \
	-nographic \
	-serial mon:stdio \
	-bios none \
	-kernel $(OUT) \
	-drive if=none,format=raw,file=$(DRIVE),id=primary \
	-device virtio-blk-device,drive=primary \
	-S \
	-s

.PHONY: clean
clean:
	cargo clean
	rm -f $(OUT)
