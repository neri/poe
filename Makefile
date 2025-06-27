.PHONY: love all default clean install iso full run test apps kernel

default:

clean:
	-rm -rf poe/**/target tools/target
	(cd poe/x86-pc && make clean)
	(cd poe/arm64-rpi && make clean)
	(cd poe/riscv-virt && make clean)

refresh:
	-rm -rf lib/Cargo.lock lib/target tools/target
	(cd poe/x86-pc && make refresh)
	(cd poe/arm64-rpi && make refresh)
	(cd poe/riscv-virt && make refresh)
