# POE for RISC-V64 virt machine

## Requirements

* risc-v rv64gc
* qemu (qemu-system-riscv64 -M virt)

## Kernel image

* `bin/kernel.img` is a flat binary with a Linux RISC-V Image header (as expected by `booti` of U-Boot).
* The image is position independent (linked at 0, relocated by `_start` at runtime) and may be loaded at any 4KB aligned address.

## Build Environment

* Rust nightly
* llvm

### building

```
$ make
$ make install
```

### run on QEMU

```
$ make run
```
