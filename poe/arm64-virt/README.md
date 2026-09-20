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

### Raspberry Pi

Copy `bin/kernel8.img` (the same image as `bin/kernel.img`) to the boot partition of the SD card,
with the Raspberry Pi firmware.

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
