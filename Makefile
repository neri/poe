.PHONY: love all clean install iso full run test apps

BIN			= bin/
TEMP		= temp/
BOOT_IMG	= $(BIN)boot.img
IPLS		= $(BIN)fdboot.bin $(BIN)cdboot.bin $(BIN)fmcdboot.bin
KERNEL_LD	= sys/target/i586-unknown-linux-gnu/release/kernel
KERNEL_CEF	= $(BIN)kernel.ceef
INITRD_IMG	= $(BIN)initrd.img
KERNEL_SYS	= $(BIN)kernel.sys
TARGETS		= $(IPLS) $(KERNEL_SYS)
ISO_SRC		= $(TEMP)iso
TARGET_ISO	= $(BIN)megos.iso

IMG_SOURCES	= $(KERNEL_SYS)

all: $(BIN) $(TARGETS)

clean:
	-rm -rf $(TARGETS)
	-rm -rf sys/target

$(BIN):
	mkdir -p $@

$(ISO_SRC):
	mkdir -p $@

$(BIN)cdboot.bin: boot/cdboot.asm
	nasm -f bin $< -o $@

$(BIN)fdboot.bin: boot/fdboot.asm
	nasm -f bin $< -o $@

$(BIN)fmcdboot.bin: boot/fmcdboot.asm
	nasm -f bin $< -o $@

$(BIN)loader.bin: boot/loader.asm
	nasm -f bin -I boot $< -o $@

$(KERNEL_LD): sys/kernel/src/*.rs sys/kernel/src/**/*.rs sys/kernel/src/**/**/*.rs lib/megstd/src/*.rs lib/megstd/src/**/*.rs
	(cd sys; cargo build -Zbuild-std --release)

$(KERNEL_CEF): tools/elf2ceef/src/*.rs $(KERNEL_LD)
	cargo run --manifest-path ./tools/elf2ceef/Cargo.toml -- $(KERNEL_LD) $(KERNEL_CEF)

$(INITRD_IMG): tools/mkinitrd/src/*.rs $(KERNEL_CEF)
	cargo run --manifest-path ./tools/mkinitrd/Cargo.toml -- $(INITRD_IMG) $(KERNEL_CEF)

$(KERNEL_SYS): $(BIN)loader.bin $(INITRD_IMG)
	cat $^ > $@

$(BOOT_IMG): install

install: tools/mkfdfs/src/*.rs $(IPLS) $(IMG_SOURCES)
	cargo run --manifest-path ./tools/mkfdfs/Cargo.toml -- -bs $(BIN)fdboot.bin $(BOOT_IMG) $(IMG_SOURCES)

full: install iso
	cargo run --manifest-path ./tools/mkfdfs/Cargo.toml -- -bs $(BIN)fdboot.bin -f 1232 $(BIN)boot.hdm $(IMG_SOURCES)
	cargo run --manifest-path ./tools/mkfdfs/Cargo.toml -- -bs $(BIN)fdboot.bin -f 160 $(BIN)mini.img $(IMG_SOURCES)

iso: $(ISO_SRC) $(IPLS) $(IMG_SOURCES)
	cp -r $(BIN)cdboot.bin $(IMG_SOURCES) $(ISO_SRC)
	mkisofs -iso-level 2 -V "MEG-OS" \
		-hide-joliet boot.catalog -hide-joliet-trans-tbl -J \
		-r -T \
		-hide boot.catalog -no-emul-boot -b cdboot.bin \
		-o $(TARGET_ISO) $(ISO_SRC)
	dd conv=notrunc if=$(BIN)fmcdboot.bin of=$(TARGET_ISO)

# run: install
