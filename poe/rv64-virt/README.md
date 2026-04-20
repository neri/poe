# POE for RISC-V64 virt machine

## Requirements

* risc-v rv64gc
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
