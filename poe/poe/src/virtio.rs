//! Small VirtIO diagnostics for QEMU's UART shell.
use minios::io::fs::media::{BlockDevice, LBA};
use minios::io::virtio::{block, rng};
use minios::prelude::*;

fn parse(value: Option<&str>) -> Option<u64> {
    value?.parse().ok()
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &byte in bytes {
        crc ^= byte as u32;
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320 & (0u32.wrapping_sub(crc & 1)));
        }
    }
    !crc
}

pub fn command<'a>(name: &str, mut args: impl Iterator<Item = &'a str>) {
    match name {
        "virq" => {
            let count = minios::io::virtio::interrupt_count();
            println!("virq: {} ({})", count, if count > 0 { "active" } else { "idle" });
        }
        "vrng" => {
            let mut bytes = [0; 16];
            match rng::fill(&mut bytes) {
                Ok(()) => println!("vrng: {:02x?}", bytes),
                Err(e) => println!("vrng: {e}"),
            }
        }
        "vblk" => {
            for index in 0..block::count() {
                if let Some(disk) = unsafe { block::device(index) } {
                    let info = disk.media_info();
                    println!(
                        "vblk{}: {} blocks x {} bytes",
                        index, info.block_count.0, info.block_size
                    );
                }
            }
        }
        "vread" | "vwrite" => {
            let Some(index) = parse(args.next()) else {
                println!("usage: {name} <disk> <lba> [count]");
                return;
            };
            let Some(lba) = parse(args.next()) else {
                println!("usage: {name} <disk> <lba> [count]");
                return;
            };
            let count = parse(args.next()).unwrap_or(1);
            if count == 0 || count > 512 {
                println!("{name}: count must be 1..=512");
                return;
            }
            let Some(disk) = (unsafe { block::device(index as usize) }) else {
                println!("{name}: no disk");
                return;
            };
            let block_size = disk.media_info().block_size as usize;
            let mut bytes = Vec::new();
            bytes.resize(block_size * count as usize, 0);
            if name == "vwrite" {
                bytes.fill(0x5a);
            }
            let result = if name == "vwrite" {
                disk.write(LBA(lba), &bytes)
            } else {
                disk.read(LBA(lba), &mut bytes)
            };
            match result {
                Ok(()) if name == "vread" => println!(
                    "vread: {} bytes first {:02x?} crc32 {:08x}",
                    bytes.len(),
                    &bytes[..bytes.len().min(16)],
                    crc32(&bytes),
                ),
                Ok(()) => println!("vwrite: {} bytes", bytes.len()),
                Err(e) => println!("{name}: {:?}", e),
            }
        }
        "vflush" | "vreset" => {
            let Some(index) = parse(args.next()) else {
                println!("usage: {name} <disk>");
                return;
            };
            match unsafe { block::device(index as usize) } {
                Some(disk) if name == "vreset" => println!("vreset: {:?}", disk.reset()),
                Some(disk) => println!("vflush: {:?}", disk.flush()),
                None => println!("{name}: no disk"),
            }
        }
        _ => {}
    }
}
