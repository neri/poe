# 🧸 The POE project 🧸

* An Experimental Project for Early OS Development.

## POE is Not an Operating System

* The main functions operate in Supervisor mode.
* No multitasking
* Minimal memory protection
* The system will stop when an exception occurs.
* In UEFI environments, `ExitBootServices` is not called until POE is exited.

## Platforms

* risc-v
  * rv32imac virt machine
  * rv64gc virt machine
* x86-64
  * UEFI
* x86-32
  * IBM PC Compatible
  * NEC PC-98 Series Computer
  * Fujitsu FM TOWNS
* arm64 (one kernel image for all of them)
  * Raspberry Pi 3 & 4
  * virt machine
  * RK3399 Chromebook (ASUS Chromebook Flip C101PA)

* **NOTE**: It may not work or may need to be adjusted as it has not been fully verified on actual hardware.

## Status

✅ implemented / ❌ not yet / — not applicable

### x86

| Feature           | PC/AT      | PC-98      | FM TOWNS   | UEFI (x64) |
|-------------------|:----------:|:----------:|:----------:|:----------:|
| Boot              | ✅         | ✅         | ✅         | ✅         |
| Text-mode console | ✅ CGA     | ✅         | ✅         | ✅ UEFI    |
| Serial console    | ✅ 16550*  | ❌         | ❌         | ❌         |
| Graphical console | ✅ VESA    | ✅ PEGC    | ✅ SVGA    | ✅ GOP     |
| Keyboard          | ✅ PS/2    | ✅ BIOS    | ✅         | ✅ UEFI    |
| Timer             | ✅ PIT     | ✅ PIT     | ✅ PIT     | ✅         |
| Panic handler     | ✅         | ✅         | ✅         | ❌         |
| Block device      | ✅ INT 13h | ✅ INT 1Bh | ✅ INT 93h | ✅         |

### RISC-V

| Feature           | RV64 virt       | RV32 virt | VisionFive 2 |
|-------------------|:---------------:|:---------:|:------------:|
| Boot              | ✅              | ✅        | ✅           |
| Text-mode console | —               | —         | —            |
| Serial console    | ✅ SBI          | ✅ SBI    | ✅ SBI       |
| Graphical console | ✅ VirtIO GPU   | —         | ✅ HDMI      |
| Keyboard          | ✅ USB          | ❌        | ✅ USB       |
| Timer             | ✅              | ✅        | ✅           |
| Panic handler     | ✅              | ✅        | ✅           |
| Block device      | ✅ USB / VirtIO | ❌        | ✅ USB       |

### Arm64

| Feature           | virt            | RPi 3/4       | Chromebook bob |
|-------------------|:---------------:|:-------------:|:--------------:|
| Boot              | ✅              | ✅            | ✅             |
| Text-mode console | —               | —             | —              |
| Serial console    | ✅ PL011        | ✅ PL011     | ❌             |
| Graphical console | ✅ VirtIO GPU   | ✅ mailbox    | ✅ coreboot FB |
| Keyboard          | ✅ USB          | ✅ USB        | ✅ ChromeOS EC |
| Timer             | ✅              | ✅            | ✅             |
| Panic handler     | ✅              | ✅            | ✅             |
| Block device      | ✅ USB / VirtIO | ✅ USB        | ❌             |

* RV32 virt runs on minisbi, the bundled SBI implementation.
* The PC/AT 16550 serial console is implemented but disabled in the default build.
* USB mass storage is read-only; VirtIO block devices support reads and writes.
* Raspberry Pi 5 is not supported.

### Not yet

* [ ] Better memory manager
* [ ] Paging
* [ ] Running after `ExitBootServices` on UEFI
* [ ] Filesystem

## History

### 2025-06-11

* Renewal

### 2021-01-06

* Initial Commit

## License

MIT License

&copy; 2002, 2021 MEG-OS project
