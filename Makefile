.PHONY: love all default clean install iso full run test apps kernel refresh

default:

clean:
	-rm -rf poe/**/target tools/target lib/target
	(cd poe/x86-pc && make clean)
	(cd poe/arm64-rpi && make clean)
	(cd poe/riscv-virt && make clean)

refresh:
	-rm -rf lib/Cargo.lock lib/target tools/target tools/Cargo.lock
	(cd poe/x86-pc && make refresh)
	(cd poe/arm64-rpi && make refresh)
	(cd poe/riscv-virt && make refresh)

test:
# 	(cd lib; cargo test)
	(cd lib/edid; cargo test)
	(cd lib/elf; cargo test)
	(cd lib/fdt; cargo test)
	(cd lib/guid; cargo test)
	(cd lib/hid; cargo test)
	(cd lib/mar; cargo test)
	(cd lib/minilib; cargo test)
	(cd lib/smbios; cargo test)
	(cd lib/uuid; cargo test)
	(cd tools; cargo test --all-features)
