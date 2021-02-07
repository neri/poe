# codename TOE

A hobby OS written in Rust, currently a subset of MYOS.

## Features

- A hobby OS written in Rust
- Not a POSIX clone system

## Requirements

- IBM PC compatible / 日本電気 PC-9800 ｼﾘｰｽﾞ ﾊﾟｰｿﾅﾙ ｺﾝﾋﾟｭｰﾀ / 富士通 FM TOWNS
- 486SX or later
- 3.6MB? or a lot more memory
- VGA or better video adapter
  - 640 x 480 pixel resolution
  - 256 color mode
- Standard keyboard and mouse
- 8253/8254 Sound
- Standard floppy drive

### Will it work with real hardware?

- Not tested.

## Build Environment

* Rust nightly
* llvm (ld.lld)
* nasm

### how to build

```
$ make
```

### how to run

```
$ make run
```

## History

### 2021-01-06

- Initial Commit

## License

MIT License

&copy; 2002, 2021 MEG-OS project
