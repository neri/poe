# POE for RISC-V32 virt machine

## Requirements

* risc-v rv32imac
* qemu (qemu-system-riscv32 -M virt)

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

### VirtIO and USB

The VirtIO MMIO and PCI xHCI drivers are shared with rv64-virt. `make run`
opens a VirtIO GPU window with a USB keyboard attached. `make run-virtio-gpu`,
`make run-virtio-blk DISK=/path/to/disk.img`, `make run-virtio-rng`,
`make run-usb` and `make run-usb-msc DISK=/path/to/disk.img` start QEMU with
the matching device, and `make test-virtio` runs the shared VirtIO tests. The
xHCI BAR is placed in the 32-bit PCI window, and DMA buffers and the GPU
framebuffer come from the 32-bit early RAM span. Press Ctrl-A, X to leave
QEMU's `-nographic` terminal.
