.PHONY: love all default clean install iso full run test apps kernel refresh

BIN			= ./bin
TEMP		= ../../temp
TOOLS		= ../../tools
ISO_SRC		= $(TEMP)/iso

ASM2BIN		= nasm -f bin
MKFDFS		= cargo run --manifest-path $(TOOLS)/Cargo.toml -p mkfdfs --
ELF2BIN		= cargo run --manifest-path $(TOOLS)/Cargo.toml -p elf2ceef --
MKINITRD	= cargo run --manifest-path $(TOOLS)/Cargo.toml -p mkinitrd --
MKKRNIMG	= cargo run --manifest-path $(TOOLS)/Cargo.toml -p mkkrnimg --
OBJDUMP		= llvm-objdump -d -M intel

TARGET_FD	= $(BIN)/bootfd.img
TARGET_FD2	= $(BIN)/bootfd.hdm
TARGET_FD3	= $(BIN)/minifd.img
TARGET_ISO	= $(BIN)/bootcd.iso
TARGET_ALL	= $(TARGET_FD) $(TARGET_FD2) $(TARGET_FD3) $(TARGET_ISO)

IPLS		= $(BIN)/fdipl.bin $(BIN)/cdipl.bin $(BIN)/fmcdipl.bin
POE_SRC		= ./poe-x86
POE_LD		= $(POE_SRC)/target/i586-unknown-none/release/poe-x86
POE_CEF		= $(BIN)/poe.cef
POE_BIN		= $(BIN)/osldr.sys
TARGETS		= $(IPLS) poe $(POE_BIN) 

IMG_SOURCES	= $(POE_BIN)
FD_DEPS		= $(BIN) $(TOOLS)/mkfdfs/src/*.rs poe $(IPLS) $(IMG_SOURCES)
ISO_DEPS	= $(BIN) $(ISO_SRC) $(IPLS) $(IMG_SOURCES)

default: $(BIN) $(TARGETS)

all: test $(TARGET_ALL)

install: test $(TARGET_FD)

iso: $(TARGET_ISO)

clean:
	(cd $(POE_SRC) && cargo clean)
	-rm -rf $(TARGETS) $(POE_LD)

refresh: clean
	-rm **/Cargo.lock

run:
	qemu-system-i386 -machine q35 -boot d -rtc base=localtime,clock=host -serial mon:stdio \
	-fda $(TARGET_FD) \
	-device nec-usb-xhci,id=xhci \
	-drive if=none,id=stick,format=raw,file=fat:rw:$(BIN) -device usb-storage,bus=xhci.0,port=2,drive=stick

#	-cdrom $(TARGET_ISO) \

run_cd:
	qemu-system-i386 -cdrom $(TARGET_ISO) -rtc base=localtime,clock=host -serial mon:stdio

run_headless:
	qemu-system-i386 -boot a -rtc base=localtime,clock=host -nographic \
	-fda $(TARGET_FD) \
	-device nec-usb-xhci,id=xhci \
	-drive if=none,id=stick,format=raw,file=fat:rw:$(BIN) -device usb-storage,bus=xhci.0,port=2,drive=stick \

# Check for use of i486 or later instructions
test: poe
	if ($(OBJDUMP) $(POE_LD) | grep -wi "bswap"); then false; fi
	if ($(OBJDUMP) $(POE_LD) | grep -wi "cmpxchg"); then false; fi
	if ($(OBJDUMP) $(POE_LD) | grep -wi "xadd"); then false; fi

$(BIN):
	mkdir -p $@

$(ISO_SRC):
	mkdir -p $@

$(BIN)/cdipl.bin: ssbl/cdipl.asm
	$(ASM2BIN) $< -o $@

$(BIN)/fdipl.bin: ssbl/fdipl.asm
	$(ASM2BIN) $< -o $@

$(BIN)/fmcdipl.bin: ssbl/fmcdipl.asm
	$(ASM2BIN) $< -o $@

$(BIN)/ssbl.bin: ssbl/ssbl.asm
	$(ASM2BIN) -I boot $< -o $@

poe:
	(cd $(POE_SRC) && cargo build --release)

$(POE_CEF): $(POE_LD) poe
	$(ELF2BIN) -v1 $(POE_LD) $(POE_CEF)

$(POE_BIN): $(BIN)/ssbl.bin $(POE_CEF)
	cat $^ > $@

$(TARGET_FD): $(FD_DEPS) 
	$(MKFDFS) -bs $(BIN)/fdipl.bin -l "poe 2hd" $(TARGET_FD) $(IMG_SOURCES)

$(TARGET_FD2): $(FD_DEPS) 
	$(MKFDFS) -bs $(BIN)/fdipl.bin -f 1232 -l "poe 1_2" $(TARGET_FD2) $(IMG_SOURCES)

$(TARGET_FD3): $(FD_DEPS) 
	$(MKFDFS) -bs $(BIN)/fdipl.bin -f 320 -l "poe 320" $(TARGET_FD3) $(IMG_SOURCES)

$(TARGET_ISO): $(ISO_DEPS)
	cp -r $(BIN)/cdipl.bin $(IMG_SOURCES) $(ISO_SRC)
	mkisofs -iso-level 2 -V "MEG-OS" \
		-hide-joliet boot.catalog -hide-joliet-trans-tbl -J \
		-r -T \
		-hide boot.catalog -no-emul-boot -b cdipl.bin \
		-o $(TARGET_ISO) $(ISO_SRC)
	dd conv=notrunc if=$(BIN)/fmcdipl.bin of=$(TARGET_ISO)
