# POE for arm64

One kernel image (`bin/kernel.img`) for 64-bit Arm machines with a device tree.
Devices are discovered from the device tree passed by the boot loader.

## Requirements

* QEMU `virt` machine (GICv2 or GICv3, PL011 UART)
* Raspberry Pi 3 / Raspberry Pi Zero 2 / Raspberry Pi 4 (Raspberry Pi 5 is not supported)
  * The console is on UART0 (PL011, GPIO 14/15, 115200 bps) and the screen (graphics console).
* Chromebooks with RK3399 (ASUS Chromebook Flip C101PA)
* Without UART in the device tree, the console output is discarded.
* On Chromebooks, the display backlight (turned off by the firmware before booting the OS) is turned on
  through the ChromeOS EC on SPI.
* On Chromebooks, the built-in keyboard (scanned by the ChromeOS EC) is used as the input in preference to UART.
  The layout is decided from the VPD like ChromeOS (`keyboard_layout`, or `region`): Japanese (JIS) or US.
  Keys are not repeated.
* If the coreboot table (Chromebooks) has a 32bpp xRGB framebuffer, it is used as the graphics console
  (`BGRX8888`, only the mode set by the firmware). Other pixel formats stay in text mode.
* The kernel image has a Linux arm64 Image header.
* The image is position independent and can be loaded at any 4KB aligned address.

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

Use `make run-el2` to boot at EL2, and `make run GIC=3` to use GICv3.

```
$ make run-rpi3
```

The Raspberry Pi 3 target attaches an emulated USB hub and Boot Protocol
keyboard. The serial log should contain `USB: hub ...` followed by
`USB: Boot keyboard ready ...`; the keyboard can then operate the POE menu.
Keep the QEMU display open because Raspberry Pi framebuffer initialization is
part of normal boot. QEMU validates enumeration and interrupt flow, but not the
Pi firmware/PHY or high-speed-hub split transactions used by the real LAN9514.

`make run-rpi4` needs QEMU 9.0 or later. The device trees of Raspberry Pi are in `dtb/`.

```
$ make FEATURES=minios/usb_debug
$ make run-xhci
```

The xHCI target is the `virt` machine with `qemu-xhci` and a Boot Protocol
keyboard, and is where the Raspberry Pi 4 USB work is developed: QEMU's
`raspi4b` has no PCIe root port, so it cannot exercise the controller behind
the Pi 4's USB-A ports. See `docs/USB_HOST_RPI4_PLAN.md`.

The keyboard operates the POE menu and shell, the same way the Raspberry Pi 3
target's does. The serial log should contain `xHCI self test: No-Op command
completed`, then `boot keyboard on interface ...`, then either `xHCI completion
mode: interrupt (IRQ n)` or `polling`. Build with `FEATURES=minios/usb_debug`
to see any of it; without that feature the stack is silent but still works.

Hot-plug, removal and moving the keyboard to another port are all handled, on
a root port or behind one tier of USB 2.0 hub (`-device usb-hub,bus=xhci.0`
with the keyboard on, say, `port=1.1`). Up to four keyboard interfaces feed
the console at once; a fifth waits until one of them goes away. A hub behind a
hub and anything that is not a keyboard are enumerated once and then left
alone.

USB mass storage (Bulk-Only Transport, read-only, LUN 0) is taken on both
the Raspberry Pi 3's DWC2 and xHCI; see `docs/USB_MSC_RPI_PLAN.md`. The POE
shell has diagnostic commands for it: `usbblk` lists the devices, `usbread
<dev> <lba> [count]` reads raw blocks and prints their CRC-32, `usbwrite <dev>
<lba>` shows that writes are refused, and `usbbench <dev> <seconds> [blocks]`
reads sequentially and reports the rate and the errors. File systems are not
read. `make run-msc DISK=<raw image>` boots with a USB 2.0-only xHCI and the image
as a High Speed USB stick; `make run-msc-ss` attaches it at SuperSpeed
instead, the way a USB3 stick in a blue port of a Raspberry Pi 4 is driven.
SuperSpeed hubs are not driven. `make test-usb` includes the mass storage
scenarios (`msc-*`, `msc-superspeed` among them), which also run on the
`raspi3b` machine.

On a GICv3 machine (`make run-xhci GIC=3`) the controller falls back to
polling, because the GICv3 driver here does not route shared peripheral
interrupts yet. Input works either way.

```
$ make test-usb
```

runs the QEMU tests of the xHCI stack (`tools/usb-test.py`): typing and
modifiers, the controller behind a PCIe root port, twenty reconnects with the
memory checked each time, hot-plug and removal behind a hub, several keyboards,
the fifth-keyboard wait, a mass storage device enumerated only once, GICv3
polling, a machine without xHCI, and the Raspberry Pi 3 path. It needs
`qemu-system-aarch64` and Python 3, and rebuilds `bin/kernel.img` with
`minios/usb_debug`, so run `make` again before installing.
`make test-usb USB_TESTS="replug no-room"` runs only the scenarios named.

### Raspberry Pi

Copy `bin/kernel8.img` (the same image as `bin/kernel.img`) to the boot partition of the SD card,
with the Raspberry Pi firmware.

#### Raspberry Pi 4 / 400: USB

The image brings up the BCM2711 PCIe link and drives the VL805 xHCI behind it,
so a USB Boot Protocol keyboard on the USB-A ports (or the Pi 400's built-in
keyboard) operates the menu and shell. Up to four keyboards work at once, and
they can be plugged in and pulled out at any time, including behind a hub.
If the bring-up fails, the boot log says `USB disabled: <reason>` and the UART
console stays usable.

The controller's DMA reaches only the RAM the device tree's `dma-ranges`
gives it (the first 3 GB on the stock device tree); the stack checks that every
buffer it hands the controller lies there.

To see what happens, build with the diagnostics and capture the UART:

```
$ make FEATURES=minios/usb_debug
```

Put `enable_uart=1` in `config.txt` so the PL011 is on GPIO 14/15, and capture
the serial console at 115200 baud. The log first shows what the firmware left
the PCIe host in (between `--- BCM2711 PCIe report ---` and
`--- end of PCIe report ---`), then the link bring-up and the `xHCI ...` lines.
Each step announces itself before it touches the hardware, so a board that
stops says where. QEMU's `raspi4b` has no PCIe, so this part is only tested on
hardware.

On a Raspberry Pi 3 Model B, connect a USB Boot Protocol keyboard to the
on-board LAN9514 hub and keep UART0 available at 115200 baud. Verify cold boot,
hot-plug, removal/reconnection, modifiers and held keys; USB failure must leave
the UART console usable.

### Chromebook kernel partition (depthcharge)

```
$ sudo apt install vboot-kernel-utils cgpt
$ make kpart
```

* `bin/kernel.kpart` is a FIT image (kernel + device tree) signed with the developer keys.
  It boots only in developer mode.
* The device tree is `dts/bob.dts` (ASUS Chromebook Flip C101PA). Use `make kpart BOARD_DTB=<file>` to use another one.
* depthcharge picks the device tree matching the board (`google,<board>-rev<N>`), or the default one if nothing matches.

### Boot from a USB memory / SD card

```
$ lsblk
$ make usb DEV=/dev/sdX
```

* `DEV` is the whole disk (`/dev/sdb`, not `/dev/sdb1`). It must be removable, and it is erased after typing `yes`.
* A GPT with one ChromeOS kernel partition is created, the kpart is written, then read back and verified.
* `make usb DEV=<file>.img` creates a disk image file instead.
* On the Chromebook (developer mode), enable USB boot once with `sudo crossystem dev_boot_usb=1`.
  Insert the disk, wait a few seconds at the warning screen, then press Ctrl+U.

# VirtIO on QEMU virt

The `virtio` feature is enabled in this POE image. QEMU 11.1.1 defaults to
legacy virtio-mmio, so the VirtIO targets pass
`-global virtio-mmio.force-legacy=false`. The driver accepts modern MMIO,
split queues and QEMU virt MMIO interrupts. It discovers up to 32 enabled DT nodes and
supports RNG, one or more block devices, and one 800×600-or-smaller 2D GPU
scanout. Entropy quality depends on the QEMU RNG backend.

```sh
make run-virtio-rng
make run-virtio-blk DISK=/path/to/raw.img
make run-virtio-gpu
make test-virtio
```

`run-virtio-blk` opens the image read-only. In the POE shell, `vrng` requests
entropy, `virq` reports received VirtIO interrupts, `vblk` lists disks,
`vread <disk> <lba> [count]` reads blocks,
`vwrite <disk> <lba> [count]` writes a 0x5a pattern, `vflush <disk>`
flushes it, and `vreset <disk>` reinitializes the device. Use only a disposable image with a writable QEMU drive. The
automated test creates its own disposable image and checks host-side writes.
`vread` reports a CRC-32; the test checks transfers larger than one 128 KiB request.
The test also boots with no VirtIO device and with an unsupported balloon device,
checks interrupt delivery with GICv2 and GICv3, and runs a headless GPU screendump with
RNG and block attached; normal `run-virtio-gpu` uses Cocoa.
It uses QMP to resize a disposable block image, checks that I/O is rejected
until `vreset`, and confirms the new capacity afterward.

The CPU waits for interrupts during synchronous I/O when they are registered;
polling remains the fallback. Packed rings, hotplug, resize, and GPU
acceleration are not supported. DMA assumes the identity-mapped RAM of QEMU
`virt`. The IRQ path is validated on QEMU's single CPU configuration.
