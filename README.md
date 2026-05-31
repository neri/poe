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
* arm64
  * Raspberry Pi 3 & 4

* **NOTE**: It may not work or may need to be adjusted as it has not been fully verified on actual hardware.

## Status

* [ ] platform
  * [ ] pc-bios
  * [ ] nec pc-98
  * [ ] fm towns
  * [ ] uefi
  * [ ] riscv64 virt
  * [ ] riscv32 virt
  * [ ] raspberry pi
    * [ ] rpi3
    * [ ] rpi4
    * [ ] rpi5
* [x] arch
  * [x] x86-32
  * [ ] x86-64
  * [x] rv64gc
  * [x] rv32imac
    * [x] minisbi
  * [x] rpi
* [x] simple console i/o
  * [x] x86-32
  * [x] riscv-sbi
  * [x] rpi-uart
  * [x] uefi
  * [x] vt100
  * [x] graphical console
    * [x] console controller
    * [x] vesa driver
    * [x] pegc driver
    * [x] fmtowns-svga driver
    * [x] rpi driver
    * [x] uefi-gop driver
* [ ] memory manager
  * [ ] better memory manager
  * [ ] uefi after exit_boot_services
  * [ ] paging
* [x] simple panic handler
  * [x] x86-32
  * [ ] x86-64
  * [x] riscv64
  * [x] riscv32
  * [x] rpi
* [x] timer
  * [x] x86-32
  * [x] riscv64
  * [x] riscv32
  * [x] rpi
  * [x] uefi
* [ ] tui
* [ ] filesystem
  * [ ] block device
    * [ ] uefi
    * [ ] pc-bios
    * [ ] nec-98
    * [ ] fm towns

## History

### 2025-06-11

* Renewal

### 2021-01-06

* Initial Commit

## License

MIT License

&copy; 2002, 2021 MEG-OS project
