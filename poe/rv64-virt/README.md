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

### PCI xHCI and USB

`make run-usb` starts QEMU virt with a PCI xHCI controller and a USB Boot
keyboard. The boot log should say `xHCI: command ring ready` and
`USB keyboard ready`. `make run-usb-msc DISK=/path/to/disk.img` also attaches
a read-only raw disk. In the POE shell, `usbblk` lists it and
`usbread 0 0 8` reads eight blocks with a CRC32. Press Ctrl-A, X to leave
QEMU's `-nographic` terminal.

### VisionFive 2

The image brings up PCIe0 (VL805 xHCI) and HDMI on its own; U-Boot does not
need to initialize either. Hardware-confirmed: USB keyboard input, USB
mass-storage read and reconnect, and a 1920x1080 60 Hz HDMI graphics console
running together with USB.

Boot with a DTB that enables both PCIe ports and the HDMI controller. The
first hardware boot used a DTB with them `disabled`; MiniOS then prints
`JH7110: boot DTB disables a required controller`. A DTB compiled from the
VisionFive 2 DTS is available locally as `bin/jh7110-visionfive-v2.dtb`.

A normal boot with a monitor and USB keyboard attached prints:

```
JH7110 PCIe pcie@2b000000: 01:00.0 vendor=0x1106 device=0x3483 class=0x0c0330
JH7110 PCIe pcie@2c000000: link down
xHCI: command ring ready, polling USB ports
USB keyboard ready
JH7110 HDMI: 1920x1080@60 output enabled
```

Failures print a reason instead, e.g. `JH7110 USB disabled: ...` or
`JH7110 HDMI disabled: ...`. `JH7110 HDMI: no monitor detected` means HPD was
low when graphics mode was requested; the console then stays on serial, and
selecting graphics mode again from the POE menu retries after a monitor is
connected. A PCIe link that is already up at boot (firmware initialized it) is
reported and skipped.

Notes:

- The HDMI mode is fixed at 1080p60; EDID is not read.
- Console output goes to HDMI while graphics mode is active; it is not
  mirrored to serial. Switching to text mode clears the screen and stops the
  HDMI output; switching back reinitializes it.
- The DC8200 reads the framebuffer without snooping the CPU caches, so the
  console writes back touched scanlines through the SiFive CCACHE Flush64
  register after each update.
- For a read-only USB mass-storage check, use `usbblk` and `usbread 0 0 8` at
  the POE prompt, then repeat `usbblk` after reconnecting the device.
