.PHONY: love all clean install iso full run test apps

BIN			= bin/
TEMP		= temp/
MISC		= misc/
BOOT_IMG	= $(BIN)boot.img
IPLS		= $(BIN)fdboot.bin $(BIN)cdboot.bin $(BIN)fmcdboot.bin
KERNEL_LD	= sys/target/i586-unknown-linux-gnu/release/kernel
KERNEL_CEF	= $(BIN)kernel
INITRD_IMG	= $(BIN)initrd.img
KERNEL_SYS	= $(BIN)kernel.sys
TARGETS		= $(IPLS) $(KERNEL_SYS)
ISO_SRC		= $(TEMP)iso
TARGET_ISO	= $(BIN)megos.iso

IMG_SOURCES	= $(KERNEL_SYS)
INITRD_FILES	= $(KERNEL_CEF) $(MISC)initrd/*
INITRD_FILES2	= apps/target/wasm32-unknown-unknown/release/*.wasm
MKFDFS		= cargo run --manifest-path ./tools/mkfdfs/Cargo.toml --

all: $(BIN) $(TARGETS)

clean:
	-rm -rf $(TARGETS)
	-rm -rf sys/target apps/target

$(BIN):
	mkdir -p $@

$(ISO_SRC):
	mkdir -p $@

$(BIN)cdboot.bin: boot/pc-bios/cdboot.asm
	nasm -f bin $< -o $@

$(BIN)fdboot.bin: boot/pc-bios/fdboot.asm
	nasm -f bin $< -o $@

$(BIN)fmcdboot.bin: boot/pc-bios/fmcdboot.asm
	nasm -f bin $< -o $@

$(BIN)loader.bin: boot/pc-bios/loader.asm
	nasm -f bin -I boot $< -o $@

$(KERNEL_LD): sys/kernel/src/*.rs sys/kernel/src/**/*.rs sys/kernel/src/**/**/*.rs lib/megstd/src/*.rs lib/megstd/src/**/*.rs lib/wasm/src/*.rs
	cd sys; cargo build -Zbuild-std --release

$(KERNEL_CEF): tools/elf2ceef/src/*.rs $(KERNEL_LD)
	cargo run --manifest-path ./tools/elf2ceef/Cargo.toml -- $(KERNEL_LD) $(KERNEL_CEF)

$(INITRD_IMG): tools/mkinitrd/src/*.rs apps $(INITRD_FILES)
	cargo run --manifest-path ./tools/mkinitrd/Cargo.toml -- $(INITRD_IMG) $(INITRD_FILES) $(INITRD_FILES2)

$(KERNEL_SYS): $(BIN)loader.bin $(INITRD_IMG)
	cat $^ > $@

$(BOOT_IMG): install

apps:
	cd apps; cargo build --target wasm32-unknown-unknown --release

install: tools/mkfdfs/src/*.rs $(IPLS) $(IMG_SOURCES)
	$(MKFDFS) -bs $(BIN)fdboot.bin $(BOOT_IMG) $(IMG_SOURCES)

full: install iso
	$(MKFDFS) -bs $(BIN)fdboot.bin -f 1232 $(BIN)boot.hdm $(IMG_SOURCES)
	$(MKFDFS) -bs $(BIN)fdboot.bin -f 320 $(BIN)mini.img $(IMG_SOURCES)

iso: $(ISO_SRC) $(IPLS) $(IMG_SOURCES)
	cp -r $(BIN)cdboot.bin $(IMG_SOURCES) $(ISO_SRC)
	mkisofs -iso-level 2 -V "MEG-OS" \
		-hide-joliet boot.catalog -hide-joliet-trans-tbl -J \
		-r -T \
		-hide boot.catalog -no-emul-boot -b cdboot.bin \
		-o $(TARGET_ISO) $(ISO_SRC)
	dd conv=notrunc if=$(BIN)fmcdboot.bin of=$(TARGET_ISO)

test:
	cargo test --manifest-path lib/wasm/Cargo.toml

# run: install
