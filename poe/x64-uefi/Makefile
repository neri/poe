.PHONY: love full default clean install run apps kernel update

MNT			= ./mnt
TEMP		= ../../temp
TOOLS		= ../../tools
EFI_BOOT	= $(MNT)/efi/boot
EFI_VENDOR	= $(MNT)/efi/megos

ASM2BIN		= nasm -f bin
MKFDFS		= cargo run --manifest-path $(TOOLS)/Cargo.toml -p mkfdfs --
ELF2BIN		= cargo run --manifest-path $(TOOLS)/Cargo.toml -p elf2ceef --
MKINITRD	= cargo run --manifest-path $(TOOLS)/Cargo.toml -p mkinitrd --
MKKRNIMG	= cargo run --manifest-path $(TOOLS)/Cargo.toml -p mkkrnimg --
OBJDUMP		= llvm-objdump -d -M intel

TARGET_ISO	= $(BIN)/bootcd.iso
TARGET_ALL	= $(TARGET_FD) $(TARGET_FD2) $(TARGET_FD3) $(TARGET_ISO)

POE_SRC		= ./poe-x64

OVMF_X64		= var/ovmfx64.fd
TARGET_BOOT_EFI_X64	= $(POE_SRC)/target/x86_64-unknown-uefi/release/poe-x64.efi
BOOT_EFI_BOOT_X64	= $(EFI_BOOT)/bootx64.efi
BOOT_EFI_VENDOR_X64	= $(EFI_VENDOR)/bootx64.efi

TARGETS		= $(BOOT_EFI_BOOT_X64)
ALL_TARGETS	= poe $(TARGETS)

default: $(TARGETS)

# iso: $(TARGET_ISO)

clean:
	(cd $(POE_SRC) && cargo clean)
	-rm -rf $(TARGETS) $(POE_LD)

update: clean
	(cd $(POE_SRC) && cargo update)

run:
	qemu-system-x86_64 -cpu IvyBridge -smp 4,cores=2,threads=2 -machine q35 -rtc base=localtime,clock=host \
		-bios $(OVMF_X64) \
		-device virtio-net-pci \
		-device nec-usb-xhci,id=xhci \
		-device intel-hda -device hda-duplex \
		-device usb-hub,bus=xhci.0,port=1,id=usb-hub \
		-drive if=none,id=stick,format=raw,file=fat:rw:$(MNT) -device usb-storage,bus=xhci.0,port=2,drive=stick \
		-device usb-tablet \
		-device usb-audio,id=usb-audio \
		-serial mon:stdio

$(EFI_BOOT):
	mkdir -p $@

$(EFI_VENDOR):
	mkdir -p $@

poe:
	(cd $(POE_SRC) && cargo build --release)

$(BOOT_EFI_BOOT_X64): $(EFI_BOOT) poe
	cp $(TARGET_BOOT_EFI_X64) $@

$(BOOT_EFI_VENDOR_X64): $(EFI_VENDOR) poe
	cp $(TARGET_BOOT_EFI_X64) $@

install: test poe $(ALL_TARGETS)

test:
