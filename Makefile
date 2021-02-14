.PHONY: love all clean install full run test apps

BIN			= bin/
BOOT_IMG	= $(BIN)boot.img
IPLS		= $(BIN)fdboot.bin
KERNEL_LD	= sys/target/i586-unknown-linux-gnu/release/kernel
KERNEL_BIN	= $(BIN)kernel.bin
KERNEL_SYS	= $(BIN)kernel.sys
TARGETS		= $(IPLS) $(KERNEL_SYS)

IMG_SOURCES	= $(KERNEL_SYS)

all: $(BIN) $(TARGETS)

clean:
	-rm -rf $(TARGETS)
	-rm -rf sys/target

$(BIN):
	mkdir -p $@

$(BIN)fdboot.bin: boot/fdboot.asm
	nasm -f bin $< -o $@

$(BIN)loader.bin: boot/loader.asm
	nasm -f bin -I boot $< -o $@

$(KERNEL_LD): sys/kernel/src/*.rs sys/kernel/src/**/*.rs sys/kernel/src/**/**/*.rs
	(cd sys; cargo build -Zbuild-std --release)

$(KERNEL_BIN): tools/krnlconv/src/*.rs $(KERNEL_LD)
	cargo run --manifest-path ./tools/krnlconv/Cargo.toml -- $(KERNEL_LD) $(KERNEL_BIN)

$(KERNEL_SYS): $(BIN)loader.bin $(KERNEL_BIN)
	cat $^ > $@

$(BOOT_IMG): install

install: tools/mkfdfs/src/*.rs $(BIN)fdboot.bin $(IMG_SOURCES)
	cargo run --manifest-path ./tools/mkfdfs/Cargo.toml -- -bs $(BIN)fdboot.bin $(BOOT_IMG) $(IMG_SOURCES)

full: install
	cargo run --manifest-path ./tools/mkfdfs/Cargo.toml -- -bs $(BIN)fdboot.bin -f 1232 $(BIN)boot.hdm $(IMG_SOURCES)
	cargo run --manifest-path ./tools/mkfdfs/Cargo.toml -- -bs $(BIN)fdboot.bin -f 160 $(BIN)mini.img $(IMG_SOURCES)

run: install
