.PHONY: love all default clean install full run run-usb run-usb-msc test apps kernel update run-virtio-rng run-virtio-blk run-virtio-gpu test-virtio

BIN			= ./bin
TEMP		= ../../temp
TOOLS		= ../../tools
ISO_SRC		= $(TEMP)/iso

MKFDFS		= cargo run --manifest-path $(TOOLS)/Cargo.toml -p mkfdfs --
ELF2BIN		= cargo run --manifest-path $(TOOLS)/Cargo.toml -p elf2ceef --
MKINITRD	= cargo run --manifest-path $(TOOLS)/Cargo.toml -p mkinitrd --
MKKRNIMG	= cargo run --manifest-path $(TOOLS)/Cargo.toml -p mkkrnimg --
OBJCOPY		= llvm-objcopy
READELF		= llvm-readelf

POE_TARGET	= riscv64gc-unknown-none-elf
POE_SRC		= ./poe-rv64
POE_LD		= $(POE_SRC)/target/$(POE_TARGET)/release/poe-rv64
POE_BIN		= $(BIN)/kernel.img
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
	qemu-system-riscv64 -M virt -smp 4 \
		-kernel $(POE_BIN) \
		-global virtio-mmio.force-legacy=false -device virtio-gpu-device \
		-device qemu-xhci -device usb-kbd \
		-serial mon:stdio

run-virtio-rng: $(POE_BIN)
	qemu-system-riscv64 -M virt -smp 1 -nographic -global virtio-mmio.force-legacy=false -kernel $(POE_BIN) \
		-object rng-builtin,id=rng0 -device virtio-rng-device,rng=rng0 -serial mon:stdio

run-virtio-blk: $(POE_BIN)
	@test -n "$(DISK)" || (echo "usage: make run-virtio-blk DISK=<raw image>"; exit 1)
	qemu-system-riscv64 -M virt -smp 1 -nographic -global virtio-mmio.force-legacy=false -kernel $(POE_BIN) \
		-drive if=none,id=vblk,format=raw,file=$(DISK),readonly=on \
		-device virtio-blk-device,drive=vblk -serial mon:stdio

run-virtio-gpu: $(POE_BIN)
	qemu-system-riscv64 -M virt -smp 1 -display cocoa -serial mon:stdio -global virtio-mmio.force-legacy=false \
		-kernel $(POE_BIN) -device virtio-gpu-device

test-virtio: $(POE_BIN)
	python3 ../arm64-virt/tools/virtio-test.py --arch riscv64 --kernel $(POE_BIN)
	python3 ../arm64-virt/tools/virtio-gpu-test.py --arch riscv64 --kernel $(POE_BIN)

# RISC-V PCI/xHCI polling path. The optional mass-storage disk is opened
# read-only by QEMU; the guest exposes only read commands.
run-usb: $(POE_BIN)
	qemu-system-riscv64 -M virt -smp 1 -nographic \
		-kernel $(POE_BIN) -device qemu-xhci -device usb-kbd \
		-serial mon:stdio

run-usb-msc: $(POE_BIN)
	@test -n "$(DISK)" || (echo "usage: make run-usb-msc DISK=<raw image>"; exit 1)
	qemu-system-riscv64 -M virt -smp 1 -nographic \
		-kernel $(POE_BIN) -device qemu-xhci -device usb-kbd \
		-drive if=none,id=stick,file=$(DISK),format=raw,readonly=on \
		-device usb-storage,drive=stick -serial mon:stdio

test: poe

$(BIN):
	mkdir -p $@

poe: $(POE_SRC)/src/link.ld
	(cd $(POE_SRC) && cargo build --release --target $(POE_TARGET))

# _start only applies R_RISCV_RELATIVE, so reject any other relocation type
$(POE_BIN): poe $(BIN)
	@if $(READELF) -r $(POE_LD) | grep R_RISCV_ | grep -qv R_RISCV_RELATIVE; then \
		echo "error: unsupported relocation type in $(POE_LD)"; exit 1; fi
	$(OBJCOPY) -O binary $(POE_LD) $(POE_BIN)
