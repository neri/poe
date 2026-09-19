.PHONY: love all default clean install full run test apps kernel update kpart usb

BIN			= ./bin
TEMP		= ../../temp
TOOLS		= ../../tools
OBJCOPY		= llvm-objcopy
READELF		= llvm-readelf
DTC			= dtc

# vboot tools and developer keys (Debian/Ubuntu: vboot-kernel-utils)
VBUTIL_KERNEL	?= vbutil_kernel
CGPT		?= cgpt
DEVKEYS		?= /usr/share/vboot/devkeys

POE_SRC		= ./poe-arm64
POE_LD		= $(POE_SRC)/target/aarch64-unknown-none/release/poe-arm64
POE_BIN		= $(BIN)/kernel.img
TARGETS		= poe $(POE_BIN)

IMG_SOURCES	= $(POE_BIN)

# DIAG=1: boot progress markers on the screen of RK3399 Chromebooks (minios/src/platform/virt/diag.rs)
CARGO_FEATURES	= $(if $(DIAG),--features diag)

# GIC version of the QEMU virt machine (2 or 3)
GIC			?= 2

# Kernel partition image for Chromebooks (depthcharge)
# BOARD_DTB can be replaced with a full device tree blob of the board.
BOARD_DTS	= ./dts/bob.dts
BOARD_DTB	?= $(BIN)/bob.dtb
FIT_ITS_IN	= ./dts/fit.its.in
FIT_ITS		= $(BIN)/kernel.its
FIT_ITB		= $(BIN)/kernel.itb
KPART		= $(BIN)/kernel.kpart

default: $(BIN) $(TARGETS)

all: test default

install: $(POE_BIN)

clean:
	(cd $(POE_SRC) && cargo clean)
	-rm -rf $(TARGETS) $(POE_LD) $(BIN)/bob.dtb $(FIT_ITS) $(FIT_ITB) $(KPART) $(BIN)/kpart.*

update: clean
	(cd $(POE_SRC) && cargo update)

run:
	qemu-system-aarch64 -M virt,gic-version=$(GIC) -cpu cortex-a53 -m 512M \
		-nographic \
		-kernel $(POE_BIN)

# EL2 entry (like depthcharge / U-Boot on real hardware)
run-el2:
	qemu-system-aarch64 -M virt,gic-version=$(GIC),virtualization=on -cpu cortex-a53 -m 512M \
		-nographic \
		-kernel $(POE_BIN)

test: poe

$(BIN):
	mkdir -p $@

poe:
	(cd $(POE_SRC) && cargo build --release $(CARGO_FEATURES))

# _start only applies R_AARCH64_RELATIVE, so reject any other relocation type
$(POE_BIN): poe $(BIN)
	@if $(READELF) -r $(POE_LD) | grep R_AARCH64_ | grep -qv R_AARCH64_RELATIVE; then \
		echo "error: unsupported relocation type in $(POE_LD)"; exit 1; fi
	$(OBJCOPY) -O binary $(POE_LD) $(POE_BIN)

kpart: $(KPART)

$(BIN)/bob.dtb: $(BOARD_DTS) | $(BIN)
	$(DTC) -I dts -O dtb -o $@ $<

$(FIT_ITB): $(FIT_ITS_IN) $(POE_BIN) $(BOARD_DTB)
	sed -e 's|@KERNEL@|$(abspath $(POE_BIN))|' -e 's|@DTB@|$(abspath $(BOARD_DTB))|' $(FIT_ITS_IN) > $(FIT_ITS)
	$(DTC) -I dts -O dtb -o $@ $(FIT_ITS)

# Signed with the developer keys, bootable in developer mode.
# The command line and the bootloader stub are required by vbutil_kernel but not used by POE.
$(KPART): $(FIT_ITB)
	echo "poe" > $(BIN)/kpart.cmdline
	head -c 512 /dev/zero > $(BIN)/kpart.bootloader
	$(VBUTIL_KERNEL) --pack $@ \
		--keyblock $(DEVKEYS)/kernel.keyblock \
		--signprivate $(DEVKEYS)/kernel_data_key.vbprivk \
		--version 1 --arch arm \
		--vmlinuz $(FIT_ITB) \
		--config $(BIN)/kpart.cmdline \
		--bootloader $(BIN)/kpart.bootloader
	$(VBUTIL_KERNEL) --verify $@ --signpubkey $(DEVKEYS)/kernel_subkey.vbpubk

# Write the kpart to a USB memory / SD card (DEV=/dev/sdX), or a disk image file (DEV=<file>.img).
# The target is erased after confirmation. See tools/write-kpart.sh for the safety checks.
usb: $(KPART)
	@test -n "$(DEV)" || { echo "usage: make usb DEV=/dev/sdX  (whole removable disk, see lsblk)"; \
		echo "       make usb DEV=<file>.img  (disk image file)"; exit 1; }
	CGPT=$(CGPT) VBUTIL_KERNEL=$(VBUTIL_KERNEL) DEVKEYS=$(DEVKEYS) ./tools/write-kpart.sh $(KPART) $(DEV)
