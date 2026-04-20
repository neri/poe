# POE for RISC-V32 virt machine

## Requirements

* risc-v rv32imac
* qemu (qemu-system-riscv64 -M virt)

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
