.PHONY: love all default clean install iso full run test apps kernel

default:

clean:
	-rm -rf boot/**/target tools/target
	(cd boot/x86-pc && make clean)
	(cd boot/arm64-rpi && make clean)
	(cd boot/riscv-virt && make clean)

refresh:
	-rm -rf lib/Cargo.lock lib/target tools/target
	(cd boot/x86-pc && make refresh)
	(cd boot/arm64-rpi && make refresh)
	(cd boot/riscv-virt && make refresh)
