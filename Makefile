.PHONY: love all clean install run test apps

BIN			= bin/
IMAGE		= $(BIN)boot.img
KERNEL_SYS	= $(BIN)kernel.sys
IPLS		= $(BIN)fdboot.bin $(BIN)fdipl.bin
TARGETS		= $(IPLS)

all: $(BIN) $(TARGETS)

clean:
	-rm -rf $(TARGETS)

$(BIN):
	mkdir -p $@

$(BIN)fdboot.bin: boot/fdboot.asm
	nasm -f bin $< -o $@

$(BIN)fdipl.bin: boot/fdipl.asm
	nasm -f bin $< -o $@

$(IMAGE): $(BIN) $(BIN)fdboot.bin
	mformat -C -i $@ -f 1440 -B $(BIN)fdboot.bin

$(BIN)kernel.sys: boot/stage2.asm
	nasm -f bin -I boot $< -o $@

install: $(IMAGE) $(KERNEL_SYS)
	mcopy -D o -i $(IMAGE) $(KERNEL_SYS) ::
