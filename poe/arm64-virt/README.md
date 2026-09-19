# POE for arm64 virt

## Requirements

* QEMU `virt` machine (GICv2 or GICv3, PL011 UART)
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

### Diagnostic build (RK3399 Chromebooks)

```
$ make usb DEV=/dev/sdX DIAG=1
```

Each boot stage draws a white band on the screen (from the top), so the stage where the boot stops can be seen
without UART. See `minios/src/platform/virt/diag.rs` for the meaning of the bands.
It uses the hardcoded VOP address of RK3399; do not use it on other machines.
