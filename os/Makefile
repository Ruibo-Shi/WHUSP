# Building
TARGET := riscv64gc-unknown-none-elf
MODE := release
KERNEL_ELF := target/$(TARGET)/$(MODE)/os
KERNEL_BIN := $(KERNEL_ELF).bin
DISASM_TMP := target/$(TARGET)/$(MODE)/asm
FS_IMG := ../user/target/$(TARGET)/$(MODE)/fs.img
ROOTFS_DIR := ../user/target/$(TARGET)/$(MODE)/rootfs
APPS := ../user/src/bin/*
FS_BLOCK_SIZE := 4096
FS_SIZE_MIB := 128
FS_BLOCK_COUNT := 32768

# BOARD
BOARD := qemu

# GUI
GUI ?= off
ifeq ($(GUI), off)
	GUI_OPTION := -display none
endif

# Building mode argument
ifeq ($(MODE), release)
	MODE_ARG := --release
endif

# KERNEL ENTRY
KERNEL_ENTRY_PA := 0x80200000

# Binutils
OBJDUMP := rust-objdump --arch-name=riscv64
OBJCOPY := rust-objcopy --binary-architecture=riscv64

# Disassembly
DISASM ?= -x

# Run usertests or usershell
TEST ?=

build: env $(KERNEL_BIN) fs-img 

env:
	(rustup target list | grep "riscv64gc-unknown-none-elf (installed)") || rustup target add $(TARGET)
	command -v rust-objcopy >/dev/null 2>&1 || cargo install cargo-binutils
	(rustup component list --installed | grep '^rust-src') >/dev/null 2>&1 || rustup component add rust-src
	(rustup component list --installed | grep '^llvm-tools-preview') >/dev/null 2>&1 || rustup component add llvm-tools-preview

$(KERNEL_BIN): kernel
	@$(OBJCOPY) $(KERNEL_ELF) --strip-all -O binary $@

fs-img: $(APPS)
	@cd ../user && make build TEST=$(TEST)
	@rm -rf $(ROOTFS_DIR)
	@mkdir -p $(ROOTFS_DIR)
	@for app in $(APPS); do \
		name=$$(basename $$app .rs); \
		cp ../user/target/$(TARGET)/$(MODE)/$$name $(ROOTFS_DIR)/$$name; \
	done
	@rm -f $(FS_IMG)
	@truncate -s $(FS_SIZE_MIB)M $(FS_IMG)
	@mke2fs -q -t ext4 -b $(FS_BLOCK_SIZE) -O ^orphan_file,^metadata_csum_seed -d $(ROOTFS_DIR) -F $(FS_IMG) $(FS_BLOCK_COUNT)

$(APPS):

kernel:
	@echo Platform: $(BOARD)
	@cp src/linker-$(BOARD).ld src/linker.ld
	@cargo build --release
	@rm src/linker.ld

clean:
	@cargo clean

disasm: kernel
	@$(OBJDUMP) $(DISASM) $(KERNEL_ELF) | less

disasm-vim: kernel
	@$(OBJDUMP) $(DISASM) $(KERNEL_ELF) > $(DISASM_TMP)
	@nvim $(DISASM_TMP)
	@rm $(DISASM_TMP)

run: run-inner
#qemu-system-riscv64 -machine virt -kernel {os_file} -m {mem} -nographic -smp {smp} -bios default -drive file={fs},if=none,format=raw,id=x0 \
                    -device virtio-blk-device, drive=x0,bus=virtio-mmio-bus.0 -no-reboot -device virtio-net-device,netdev=net -netdev user,id=net \
                    -rtc base=utc \
                    -drive file=disk.img,if=none,format=raw,id=x1 -device virtio-blk-device,drive=x1,bus=virtio-mmio-bus.1
QEMU_ARGS := -machine virt \
			 -bios default \
			 -serial stdio \
			 $(GUI_OPTION) \
			 -kernel $(KERNEL_ELF) \
			 -drive file=$(FS_IMG),if=none,format=raw,id=x0 \
			 -device virtio-blk-device,drive=x0 \
			 -device virtio-gpu-device \
			 -device virtio-keyboard-device \
			 -device virtio-mouse-device \
			 -device virtio-net-device,netdev=net0 \
			 -netdev user,id=net0,hostfwd=udp::6200-:2000,hostfwd=tcp::6201-:80

fdt:
	@qemu-system-riscv64 -M 128m -machine virt,dumpdtb=virt.out
	fdtdump virt.out

QEMU_NAME := qemu-system-riscv64
qemu-version-check:
	@sh scripts/qemu-ver-check.sh $(QEMU_NAME)

run-inner: qemu-version-check build
	@qemu-system-riscv64 $(QEMU_ARGS)

debug: qemu-version-check build
	@tmux new-session -d \
		"qemu-system-riscv64 $(QEMU_ARGS) -s -S" && \
		tmux split-window -h "riscv64-unknown-elf-gdb -ex 'file $(KERNEL_ELF)' -ex 'set arch riscv:rv64' -ex 'target remote localhost:1234'" && \
		tmux -2 attach-session -d

gdbserver: qemu-version-check build
	@qemu-system-riscv64 $(QEMU_ARGS) -s -S

gdbclient:
	@riscv64-unknown-elf-gdb -ex 'file $(KERNEL_ELF)' -ex 'set arch riscv:rv64' -ex 'target remote localhost:1234'

.PHONY: build env kernel clean disasm disasm-vim run-inner fs-img gdbserver gdbclient fdt qemu-version-check
