################
# BUILD
################
CC=riscv64-linux-gnu-gcc
CFLAGS=-std=c++17 -Wall -Wextra -pedantic -O0 -g
CFLAGS+=-static -nostdlib -ffreestanding -fno-rtti -fno-exceptions
CFLAGS+=-march=rv64gc -mabi=lp64d
LINKER_SCRIPT=-Tsrc/lds/virt.ld
RUST_PROFILE=dev
ifeq ($(RUST_PROFILE),dev)
	BUILD_CMD=cargo build
	RUST_TARGET=./target/riscv64gc-unknown-none-elf/debug
else
	BUILD_CMD=cargo build --$(RUST_PROFILE)
	RUST_TARGET=./target/riscv64gc-unknown-none-elf/$(RUST_PROFILE)
endif
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
	$(BUILD_CMD)
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
	-drive if=none,format=raw,file=$(DRIVE),id=primary \
	-device virtio-blk-device,drive=primary \
	-kernel $(OUT)

dbg: all
	$(QEMU) \
	-machine $(MACH) \
	-m $(MEM) \
	-cpu $(CPU) \
	-smp $(CPUS) \
	-nographic \
	-serial mon:stdio \
	-bios none \
	-drive if=none,format=raw,file=$(DRIVE),id=primary \
	-device virtio-blk-device,drive=primary \
	-kernel $(OUT) \
	-S \
	-s

.PHONY: clean
clean:
	cargo clean
	rm -f $(OUT)

