# MEG-OS IPL boot protocol specification

Status: Draft

Copyright (c) 2026 MEG-OS Project

## First Stage Boot Loader (FSBL aka IPL)

* Load a binary with a name like "KERNEL.SYS" or "OSLDR.SYS" from the boot disk.

### State at transition from FSBL to SSBL

* REAL MODE
* CS:IP = `0x1000`:`0x0000`
* AX = signature (`0x1eaf`)
* CL = platform type
  * `0x00` NEC PC-98
  * `0x01` IBM PC compatible
  * `0x02` FUJITSU FM TOWNS
* CH = boot drive id
  * ex. `0x00` = Floppy on PC compatible machines

```
  0000_0000 +-------------------+
            | IDT               |
  0000_0400 +-------------------+
            | BIOS DATA AREA    |
            + - - - - - - - - - +
            | UNUSED            |
  0001_0000 +-------------------+
            | KERNEL.SYS (SSBL) |
            +-------------------+
            | UNUSED            |
  000A_0000 +-------------------+
            | VRAM & BIOS       |
  0010_0000 +-------------------+
            | UNUSED            |
            +-------------------+
```

## Second Stage Boot Loader (SSBL)

* After a simple system environment check, go to protected mode, decompresses the system image, and runs it.

### State at transition from SSBL to SYSTEM

* NON-PAGED PROTECTED MODE
* CS = 32BIT FLAT RING0
* DS,ES,SS = 32BIT FLAT RING0
* FS,GS = UNDEFINED
* EIP = CEEF_ENTRY
* ECX = boot_info
* ESP = initial stack (about 0x1000 bytes after the system)

```
  0000_0000 +---------------------+
            | IDT                 |
  0000_0400 +---------------------+
            | BIOS DATA AREA      |
  0000_0800 +---------------------+
            | SSBL (MOVED)        |
            + - - - - - - - - - - +
            | boot_info           |
            + - - - - - - - - - - +
            | SYSTEM IMAGE        |
            +---------------------+
            | UNUSED              |
  000A_0000 +---------------------+
            | VRAM & BIOS         |
  0010_0000 +---------------------+
            | SYSTEM              |
 CEEF_ENTRY + - - - - - - - - - - +
            | SYSTEM              |
            + - - - - - - - - - - +
            | INITIAL STACK       |
        ESP +---------------------+
            | AVAILABLE MEMORY    |
            +---------------------+
```

### boot_info

```
_boot_info:
_platform           db 0
_boot_drive         db 0
_memsz_lo           dw 0
_start_mid          dd 0x00100000
_memsz_mid          dd 0
```

* _platform: platform type (DIFFERENT FROM FSBL)
  * `0x01` NEC PC-98
  * `0x02` IBM PC compatible
  * `0x03` FUJITSU FM TOWNS
* _boot_drive: bios boot drive id (same as FSBL)
* _memsz_lo: memory size under 1mb (typically 0xa000)
* _start_mid: start of available extended memory
* _memsz_mid: Available memory size. This refers only to the amount of memory recognized by SSBL and available at system startup. It does not represent the total system memory.

## License

Copyright (c) 2026 MEG-OS Project

This document is licensed under CC BY-SA 4.0.
https://creativecommons.org/licenses/by-sa/4.0/
