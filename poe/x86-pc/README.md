# POE for x86

## Requirements

* Computer
  * PC Compatible
  * NEC PC-9800 Series
  * Fujitsu FM TOWNS
* 386SX? or later
* 3.6MB? or a lot more memory
* VGA or better video adapter
  * supported VESA BIOS Extension 2.0
  * supported 256 color mode on PC-9821 
* Standard keyboard and mouse
* 8253/8254 Sound (Optional)
* Standard disk drive

* **NOTE**: It may not work or may need to be adjusted as it has not been fully verified on actual hardware.

## Build Environment

* Rust nightly
* llvm
* nasm

### building

```
$ make
$ make install
```

### run on QEMU

```
$ make run
```

### make an iso image

```
$ make iso
```

### make all targets

```
$ make all
```

## final products

### bin/bootfd.img

* generic bootable floppy image

### bin/bootfd.hdm

* floppy image optimized for PC98 and FM TOWNS

### bin/bootcd.iso

* bootable cd image
