ARCH ?= riscv64
MODE := release
CARGO_HOME ?= $(abspath ../vendor)
export CARGO_HOME

ifeq ($(ARCH),riscv64)
TARGET := riscv64gc-unknown-none-elf
OBJDUMP := rust-objdump --arch-name=riscv64
OBJCOPY := rust-objcopy --binary-architecture=riscv64
else ifeq ($(ARCH),loongarch64)
$(error LoongArch user programs are not implemented yet: user syscall wrappers and ABI setup belong to P5 stage 5)
else
$(error Unsupported ARCH '$(ARCH)' in user/Makefile. Supported values are riscv64 and loongarch64)
endif

APP_DIR := src/bin
TARGET_DIR := target/$(TARGET)/$(MODE)
APPS := $(wildcard $(APP_DIR)/*.rs)
ELFS := $(patsubst $(APP_DIR)/%.rs, $(TARGET_DIR)/%, $(APPS))
BINS := $(patsubst $(APP_DIR)/%.rs, $(TARGET_DIR)/%.bin, $(APPS))

CP := cp 

TEST ?= 

elf: $(APPS)
	@cargo build --release
ifeq ($(TEST), 1)
	@$(CP) $(TARGET_DIR)/usertests $(TARGET_DIR)/initproc
endif

binary: elf
	@$(foreach elf, $(ELFS), $(OBJCOPY) $(elf) --strip-all -O binary $(patsubst $(TARGET_DIR)/%, $(TARGET_DIR)/%.bin, $(elf));)

build: binary

clean:
	@cargo clean

.PHONY: elf binary build clean
