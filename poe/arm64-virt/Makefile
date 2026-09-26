.PHONY: love all default clean install full run run-el2 run-xhci run-msc run-rpi3 run-rpi4 test test-usb apps kernel update kpart usb run-virtio-rng run-virtio-blk run-virtio-gpu test-virtio

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
# The same image, named for the Raspberry Pi firmware
RPI_BIN		= $(BIN)/kernel8.img
TARGETS		= poe $(POE_BIN) $(RPI_BIN)

IMG_SOURCES	= $(POE_BIN)


# GIC version of the QEMU virt machine (2 or 3)
GIC			?= 2

# Device tree blobs of Raspberry Pi (from the Raspberry Pi firmware)
DTB_PATH	= ./dtb

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

install: $(POE_BIN) $(RPI_BIN)

clean:
	(cd $(POE_SRC) && cargo clean)
	-rm -rf $(TARGETS) $(POE_LD) $(BIN)/bob.dtb $(FIT_ITS) $(FIT_ITB) $(KPART) $(BIN)/kpart.*

update: clean
	(cd $(POE_SRC) && cargo update)

run:
	qemu-system-aarch64 -M virt,gic-version=$(GIC) -cpu cortex-a53 -m 512M \
		-kernel $(POE_BIN) \
		-global virtio-mmio.force-legacy=false -device virtio-gpu-device \
		-device qemu-xhci,id=xhci \
		-device usb-kbd,id=kbd1,bus=xhci.0 \
		-serial mon:stdio

run-virtio-rng: $(POE_BIN)
	qemu-system-aarch64 -M virt,gic-version=$(GIC) -cpu cortex-a53 -m 512M -nographic -global virtio-mmio.force-legacy=false \
		-kernel $(POE_BIN) -object rng-builtin,id=rng0 -device virtio-rng-device,rng=rng0

run-virtio-blk: $(POE_BIN)
	@test "$(origin DISK)" = "command line" || (echo "usage: make run-virtio-blk DISK=<raw image>"; exit 1)
	qemu-system-aarch64 -M virt,gic-version=$(GIC) -cpu cortex-a53 -m 512M -nographic -global virtio-mmio.force-legacy=false \
		-kernel $(POE_BIN) -drive if=none,id=vblk,format=raw,file=$(DISK),readonly=on \
		-device virtio-blk-device,drive=vblk

run-virtio-gpu: $(POE_BIN)
	qemu-system-aarch64 -M virt,gic-version=$(GIC) -cpu cortex-a53 -m 512M -global virtio-mmio.force-legacy=false \
		-display cocoa -serial mon:stdio -kernel $(POE_BIN) -device virtio-gpu-device

test-virtio: $(POE_BIN)
	python3 tools/virtio-test.py --arch aarch64 --kernel $(POE_BIN)
	python3 tools/virtio-test.py --arch aarch64 --gic-version 3 --kernel $(POE_BIN)
	python3 tools/virtio-gpu-test.py --arch aarch64 --kernel $(POE_BIN)

# USB on xHCI, per docs/USB_HOST_RPI4_PLAN.md.  The Raspberry Pi 4 machine of
# QEMU has no PCIe root port, so the xHCI work is verified on virt instead.
# Build with FEATURES=minios/usb_debug to see what the probe found.
run-xhci:
	qemu-system-aarch64 -M virt,gic-version=$(GIC) -cpu cortex-a53 -m 512M \
		-nographic \
		-kernel $(POE_BIN) \
		-device qemu-xhci,id=xhci \
		-device usb-kbd,id=kbd1,bus=xhci.0

# USB mass storage, per docs/USB_MSC_RPI_PLAN.md: DISK is a raw image read
# through usbblk/usbread in the POE shell.  p3=0 leaves the controller USB 2.0
# only, so the stick is High Speed; run-msc-ss keeps the SuperSpeed ports and
# QEMU attaches it at SuperSpeed.
DISK ?= $(TEMP)/usb-disk.img
run-msc:
	qemu-system-aarch64 -M virt,gic-version=$(GIC) -cpu cortex-a53 -m 512M \
		-nographic \
		-kernel $(POE_BIN) \
		-device qemu-xhci,id=xhci,p3=0 \
		-device usb-kbd,id=kbd1,bus=xhci.0 \
		-drive if=none,id=stick,format=raw,file=$(DISK) \
		-device usb-storage,bus=xhci.0,drive=stick

run-msc-ss:
	qemu-system-aarch64 -M virt,gic-version=$(GIC) -cpu cortex-a53 -m 512M \
		-nographic \
		-kernel $(POE_BIN) \
		-device qemu-xhci,id=xhci \
		-device usb-kbd,id=kbd1,bus=xhci.0 \
		-drive if=none,id=stick,format=raw,file=$(DISK) \
		-device usb-storage,bus=xhci.0,drive=stick

# QEMU tests of the xHCI stack (tools/usb-test.py).  They read its diagnostic
# lines, so this rebuilds bin/kernel.img with minios/usb_debug and leaves that
# image behind; run make again without FEATURES before installing.
# USB_TESTS picks scenarios, e.g. make test-usb USB_TESTS="replug no-room"
USB_TESTS ?=
test-usb:
	$(MAKE) FEATURES=minios/usb_debug $(POE_BIN)
	python3 tools/usb-test.py --kernel $(POE_BIN) --dtb-dir $(DTB_PATH) $(USB_TESTS)

# EL2 entry (like depthcharge / U-Boot on real hardware)
run-el2:
	qemu-system-aarch64 -M virt,gic-version=$(GIC),virtualization=on -cpu cortex-a53 -m 512M \
		-nographic \
		-kernel $(POE_BIN)

run-rpi3:
	qemu-system-aarch64 -M raspi3b -dtb $(DTB_PATH)/bcm2710-rpi-3-b.dtb \
		-kernel $(POE_BIN) \
		-device usb-hub,id=hub1,bus=usb-bus.0,port=1 \
		-device usb-kbd,id=kbd1,bus=usb-bus.0,port=1.1 \
		-serial mon:stdio

# Needs QEMU 9.0 or later
run-rpi4:
	qemu-system-aarch64 -M raspi4b -dtb $(DTB_PATH)/bcm2711-rpi-4-b.dtb \
		-kernel $(POE_BIN) \
		-serial mon:stdio

test: poe

$(BIN):
	mkdir -p $@

# FEATURES passes extra cargo features, e.g. make FEATURES=minios/usb_debug
FEATURES ?=
poe:
	(cd $(POE_SRC) && cargo build --release $(if $(FEATURES),--features $(FEATURES),))

# _start only applies R_AARCH64_RELATIVE, so reject any other relocation type
$(POE_BIN): poe $(BIN)
	@if $(READELF) -r $(POE_LD) | grep R_AARCH64_ | grep -qv R_AARCH64_RELATIVE; then \
		echo "error: unsupported relocation type in $(POE_LD)"; exit 1; fi
	$(OBJCOPY) -O binary $(POE_LD) $(POE_BIN)

$(RPI_BIN): $(POE_BIN)
	cp $(POE_BIN) $(RPI_BIN)

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
