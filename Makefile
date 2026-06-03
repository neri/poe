.PHONY: love all default clean install iso full run test apps kernel update doc

default:

clean:
	-rm -rf minios/target poe/**/target tools/target lib/target
	(cd poe/arm64-rpi && make clean)
	(cd poe/rv32-virt && make clean)
	(cd poe/rv64-virt && make clean)
	(cd poe/x64-uefi && make clean)
	(cd poe/x86-pc && make clean)

update: clean
	-rm -rf minios/Cargo.lock lib/Cargo.lock lib/target tools/target tools/Cargo.lock
	(cd poe/arm64-rpi && make update)
	(cd poe/rv32-virt && make update)
	(cd poe/rv64-virt && make update)
	(cd poe/x64-uefi && make update)
	(cd poe/x86-pc && make update)

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

doc:
	(cd minios/; cargo doc --no-deps -F device_tree)
