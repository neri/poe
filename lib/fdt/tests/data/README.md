# Test data of `fdt`

| File | Source |
|---|---|
| `bus.dtb` | `bus.dts`: `dtc -q -I dts -O dtb -o bus.dtb bus.dts` (intentionally malformed nodes warn without `-q`) |
| `bcm2710-rpi-3-b.dtb`, `bcm2711-rpi-4-b.dtb` | Copies of `poe/arm64-virt/dtb/` (from the Raspberry Pi firmware) |
| `bob.dtb` | `poe/arm64-virt/dts/bob.dts`: `dtc -q -I dts -O dtb -o bob.dtb ../../../../poe/arm64-virt/dts/bob.dts` |
| `qemu-virt-gicv3.dtb` | QEMU 8.2 `-M virt,gic-version=3,dumpdtb=<file> -cpu cortex-a53 -m 512M`, compacted with `dtc -I dtb -O dtb` |
