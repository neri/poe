# MEG-OS IPL boot protocol specification

## First Stage Boot Loader (FSBL aka IPL)

* Load a binary with a name like "KERNEL.SYS" or "OSLDR.SYS" from the boot disk.

### State at transition from FSBL to SSBL

* REAL MODE
* CS:IP = `0x1000`:`0x0000`
* AX = signature (`0x1eaf`)
* CH = platform type
  * `0x01` NEC PC-98
  * `0x02` IBM PC compatible
  * `0x03` FUJITSU FM TOWNS
* CL = boot drive id
  * ex. `0x00` = Floppy on PC compatible machines

```
  0000_0000 +-------------------+
            | IDT               |
  0000_0400 +-------------------+
            | BIOS DATA AREA    |
            +-------------------+
            | UNUSED            |
  0001_0000 +-------------------+
            | SSBL              |
  SSBL+_END +-------------------+
            | KERNEL IMAGE      |
            +-------------------+
            | UNUSED            |
  000A_0000 +-------------------+
            | VRAM & BIOS       |
  0010_0000 +-------------------+
            | UNUSED            |
            +-------------------+
```

## Second Stage Boot Loader (SSBL)

* After checking the system, go to protected mode and run the first binary in the kernel image.

### State at transition from SSBL to KERNEL

* NON-PAGED PROTECTED MODE
* CS = 32BIT FLAT RING0
* DS,ES,SS = 32BIT FLAT RING0
* EIP = CEEF_ENTRY
* ECX = boot_info

```
  0000_0000 +-------------------+
            | IDT               |
  0000_0400 +-------------------+
            | BIOS DATA AREA    |
  0000_0800 +-------------------+
            | SSBL              |
            +-------------------+
            | boot_info         |
            +-------------------+
            | UNUSED            |
            +-------------------+
            | KERNEL IMAGE      |
            +-------------------+
            | UNUSED            |
  000A_0000 +-------------------+
            | VRAM & BIOS       |
  0010_0000 +-------------------+
 CEEF_ENTRY +-------------------+
            | KERNEL            |
            +-------------------+
            | UNUSED            |
            +-------------------+
```
