.PHONY: love all default clean install full run test apps kernel update

BIN			= ./bin
TEMP		= ../../temp
TOOLS		= ../../tools
ISO_SRC		= $(TEMP)/iso

MKFDFS		= cargo run --manifest-path $(TOOLS)/Cargo.toml -p mkfdfs --
ELF2BIN		= cargo run --manifest-path $(TOOLS)/Cargo.toml -p elf2ceef --
MKINITRD	= cargo run --manifest-path $(TOOLS)/Cargo.toml -p mkinitrd --
MKKRNIMG	= cargo run --manifest-path $(TOOLS)/Cargo.toml -p mkkrnimg --
OBJCOPY		= llvm-objcopy

POE_TARGET	= riscv32imac-unknown-none-elf
POE_SRC		= ./poe-rv32
POE_LD		= $(POE_SRC)/target/$(POE_TARGET)/release/poe-rv32
POE_BIN		= $(BIN)/kernel.elf
TARGETS		= poe $(POE_BIN)

IMG_SOURCES	= $(POE_BIN)

default: $(BIN) $(TARGETS)

all: test default

install: $(POE_BIN)

clean:
	(cd $(POE_SRC) && cargo clean)
	-rm -rf $(TARGETS) $(POE_LD)

update: clean
	(cd $(POE_SRC) && cargo update)

run:
	qemu-system-riscv32 -M virt -bios none \
		-nographic \
		-kernel $(POE_BIN) \
		-serial mon:stdio

# 		-device virtio-vga \

test: poe

$(BIN):
	mkdir -p $@

poe: $(POE_SRC)/src/link.ld
	(cd $(POE_SRC) && cargo build --release --target $(POE_TARGET))

$(POE_BIN): poe $(BIN)
	cp $(POE_LD) $(POE_BIN)
