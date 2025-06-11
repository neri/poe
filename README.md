# Pre-OS Execution Environment

```
()=()  |
('Y') <  hi, i'm Bare Metal Bear!
q . p  |
()_()
```

## Features

* Pre-OS Execution Environment

## Requirements

* NOTE: May not work or may need to be adjusted as it has not been fully verified on actual hardware.

### x86

* Computer
  * PC AT Compatible
  * (NEC PC-9800 Series)
  * (Fujitsu FM TOWNS)
* 386SX? or later
* 3.6MB? or a lot more memory
* VGA or better video adapter
* Standard keyboard and mouse
* (Optional) 8253/8254 Sound
* Standard disk drive

### riscv

* Computer
  * qemu (qemu-system-riscv64 -M virt)

### arm64

* Computer
  * Raspberry Pi 3/4
  * Currently, Raspberry Pi 5 is not supported

## Build Environment

* Rust nightly
* llvm (ld.lld)
* nasm

### x86

* building

```
$ cd poe/x86-pc
$ make
$ make install
```

* then run

```
$ make run
```

## History

### 2025-06-11

* Renewal

### 2021-01-06

* Initial Commit

## License

MIT License

&copy; 2002, 2021 MEG-OS project
