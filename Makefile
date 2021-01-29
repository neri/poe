.PHONY: love all clean install run test apps

BIN			= bin/
IMAGE		= $(BIN)boot.img
IPLS		= $(BIN)fdboot.bin
KERNEL_LD	= sys/target/i586-unknown-linux-gnu/release/kernel
KERNEL_BIN	= $(BIN)kernel.bin
KERNEL_SYS	= $(BIN)kernel.sys
TARGETS		= $(IPLS) $(KERNEL_SYS)

all: $(BIN) $(TARGETS)

clean:
	-rm -rf $(TARGETS)
	-rm -rf sys/target

$(BIN):
	mkdir -p $@

$(BIN)fdboot.bin: boot/fdboot.asm
	nasm -f bin $< -o $@

$(IMAGE): $(BIN) $(BIN)fdboot.bin
	mformat -C -i $@ -f 1440 -B $(BIN)fdboot.bin

$(BIN)loader.bin: boot/loader.asm
	nasm -f bin -I boot $< -o $@

$(KERNEL_LD): sys/kernel/src/*.rs sys/kernel/src/**/*.rs sys/kernel/src/**/**/*.rs
	(cd sys; cargo build -Zbuild-std --release)

$(KERNEL_BIN): tools/convert/src/*.rs $(KERNEL_LD)
	(cd tools/convert; cargo run ../../$(KERNEL_LD) ../../$(KERNEL_BIN))

$(KERNEL_SYS): $(BIN)loader.bin $(KERNEL_BIN)
	cat $^ > $@

install: $(IMAGE) $(KERNEL_SYS)
	mcopy -D o -i $(IMAGE) $(KERNEL_SYS) ::

run: install
