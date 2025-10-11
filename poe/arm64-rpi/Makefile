.PHONY: love all default clean install full run test apps kernel refresh

BIN			= ./bin
TEMP		= ../../temp
TOOLS		= ../../tools
ISO_SRC		= $(TEMP)/iso

MKFDFS		= cargo run --manifest-path $(TOOLS)/Cargo.toml -p mkfdfs --
ELF2BIN		= cargo run --manifest-path $(TOOLS)/Cargo.toml -p elf2ceef --
MKINITRD	= cargo run --manifest-path $(TOOLS)/Cargo.toml -p mkinitrd --
MKKRNIMG	= cargo run --manifest-path $(TOOLS)/Cargo.toml -p mkkrnimg --
OBJCOPY		= llvm-objcopy

POE_SRC		= ./poe-rpi
POE_LD		= $(POE_SRC)/target/aarch64-unknown-none/release/poe-rpi
POE_BIN		= $(BIN)/kernel8.img
TARGETS		= poe $(POE_BIN)

IMG_SOURCES	= $(POE_BIN)

default: $(BIN) $(TARGETS)

all: test default

install: $(POE_BIN)

clean:
	(cd $(POE_SRC) && cargo clean)
	-rm -rf $(TARGETS) $(POE_LD)

refresh: clean
	-rm **/Cargo.lock

run: run3

run3:
	qemu-system-aarch64 -M raspi3b -dtb ../../ext/dtb/bcm2710-rpi-3-b.dtb \
	-kernel $(POE_BIN) \
	-serial mon:stdio

run4:
	qemu-system-aarch64 -M raspi4b -dtb ../../ext/dtb/bcm2711-rpi-4-b.dtb \
	-kernel $(POE_BIN) \
	-serial mon:stdio

test: poe

$(BIN):
	mkdir -p $@

poe:
	(cd $(POE_SRC) && cargo build --release)

$(POE_BIN): poe $(BIN)
	$(OBJCOPY) -O binary $(POE_LD) $(POE_BIN)
