.PHONY: love all default clean install full run test apps kernel

BIN			= ./bin
TEMP		= ../../temp
TOOLS		= ../../tools
ISO_SRC		= $(TEMP)/iso

MKFDFS		= cargo run --manifest-path $(TOOLS)/Cargo.toml -p mkfdfs --
ELF2BIN		= cargo run --manifest-path $(TOOLS)/Cargo.toml -p elf2ceef --
MKINITRD	= cargo run --manifest-path $(TOOLS)/Cargo.toml -p mkinitrd --
MKKRNIMG	= cargo run --manifest-path $(TOOLS)/Cargo.toml -p mkkrnimg --
OBJCOPY		= objcopy

POE_SRC		= ./poe-rv
POE_LD		= $(POE_SRC)/target/riscv64gc-unknown-none-elf/release/poe-rv
POE_BIN		= $(BIN)/kernel.elf
TARGETS		= poe $(POE_BIN)

IMG_SOURCES	= $(POE_BIN)

default: $(BIN) $(TARGETS)

all: test default

install: $(POE_BIN)

clean:
	-rm -rf $(POE_SRC)/target
	-rm -rf $(TARGETS) $(POE_LD)

refresh: clean
	-rm **/Cargo.lock

run:
	qemu-system-riscv64 -M virt \
	-kernel $(POE_BIN) \
	-serial mon:stdio

test: poe

$(BIN):
	mkdir -p $@

poe:
	(cd $(POE_SRC) && cargo build --release)

$(POE_BIN): poe
	cp $(POE_LD) $(POE_BIN)
