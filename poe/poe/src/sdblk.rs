//! Raw SD card diagnostics (`docs/SD_BLOCK_PLAN.md`).

use alloc::string::ToString;
use alloc::vec;

use minios::io::sd;
use minios::prelude::*;

const BLOCK_SIZE: usize = 512;
const MAX_READ_BLOCKS: u64 = 256;
const TEST_CHUNK_BLOCKS: u64 = 16;

pub fn command<'a>(name: &str, mut args: impl Iterator<Item = &'a str>) {
    match name {
        "sdblk" => list(),
        "sdread" => {
            let (Some(device), Some(lba)) =
                (args.next().and_then(parse), args.next().and_then(parse))
            else {
                println!("usage: sdread <dev> <lba> [count]");
                return;
            };
            let count = args.next().and_then(parse).unwrap_or(1);
            read(device as usize, lba, count);
        }
        "sdtest" => {
            let (Some(device), Some(lba), Some(count)) = (
                args.next().and_then(parse),
                args.next().and_then(parse),
                args.next().and_then(parse),
            ) else {
                println!("usage: sdtest <dev> <lba> <count> [--destroy]");
                return;
            };
            let destroy = args.next() == Some("--destroy") && args.next().is_none();
            test(device as usize, lba, count, destroy);
        }
        "sdreset" => {
            let Some(device) = args.next().and_then(parse) else {
                println!("usage: sdreset <dev>");
                return;
            };
            reset(device as usize);
        }
        _ => {}
    }
}

fn parse(text: &str) -> Option<u64> {
    match text.strip_prefix("0x") {
        Some(hex) => u64::from_str_radix(hex, 16).ok(),
        None => text.parse().ok(),
    }
}

fn list() {
    let devices = sd::devices();
    if devices.is_empty() {
        println!("sdblk: no SD cards");
        return;
    }
    for device in devices {
        match device.info {
            Some(info) => println!(
                "sd{}: {:?} {:?} {} x {} blocks, RCA {:04x}, {}-bit, {} Hz",
                device.index,
                info.kind,
                info.addressing,
                info.capacity_blocks,
                BLOCK_SIZE,
                info.rca,
                if info.bus_width == sd::BusWidth::Four {
                    4
                } else {
                    1
                },
                info.clock_hz,
            ),
            None => println!(
                "sd{}: unavailable; run sdreset {}",
                device.index, device.index
            ),
        }
    }
}

fn read(device: usize, lba: u64, count: u64) {
    if count == 0 || count > MAX_READ_BLOCKS {
        println!("sdread: count must be 1..={}", MAX_READ_BLOCKS);
        return;
    }
    let bytes = match usize::try_from(count)
        .ok()
        .and_then(|v| v.checked_mul(BLOCK_SIZE))
    {
        Some(bytes) => bytes,
        None => {
            println!("sdread: range is too large");
            return;
        }
    };
    let Some(result) = sd::with_device(device, |media| {
        let mut buffer = vec![0; bytes];
        let (capacity, block_size) = {
            let info = media.media_info();
            (info.block_count.0, info.block_size)
        };
        if lba.checked_add(count).map_or(true, |end| end > capacity) {
            return Err("range is outside the card".to_string());
        }
        media
            .sd_read(lba, &mut buffer)
            .map(|()| (buffer, block_size))
            .map_err(|failure| {
                alloc::format!(
                    "{:?} at lba {} after {} blocks",
                    failure.error,
                    failure.failed_lba,
                    failure.completed_blocks
                )
            })
    }) else {
        println!("sdread: no sd{}", device);
        return;
    };
    match result {
        Ok((buffer, block_size)) => println!(
            "sdread: sd{} lba {} count {} block_size {} crc32 {:08x}",
            device,
            lba,
            count,
            block_size,
            crc32(&buffer)
        ),
        Err(error) => println!(
            "sdread: sd{} lba {} count {}: {}",
            device, lba, count, error
        ),
    }
}

fn test(device: usize, lba: u64, count: u64, destroy: bool) {
    let Some(result) = sd::with_device(device, |media| {
        let capacity = media.media_info().block_count.0;
        let Some(end) = lba.checked_add(count) else {
            return Err("LBA range overflows".to_string());
        };
        if count == 0 || end > capacity {
            return Err("range must be nonempty and inside the card".to_string());
        }
        if !destroy {
            println!(
                "sdtest: sd{} would overwrite lba {}..{} ({} blocks); pass --destroy to write",
                device, lba, end, count
            );
            return Ok(());
        }

        let mut done = 0;
        while done < count {
            let blocks = (count - done).min(TEST_CHUNK_BLOCKS);
            let bytes = blocks as usize * BLOCK_SIZE;
            let chunk_lba = lba + done;
            let mut expected = vec![0; bytes];
            if let Err(error) = media.sd_read(chunk_lba, &mut expected) {
                return Err(alloc::format!(
                    "read failed at lba {} after {} blocks: {:?}",
                    error.failed_lba,
                    done + error.completed_blocks,
                    error.error
                ));
            }
            for (offset, byte) in expected.iter_mut().enumerate() {
                let block_lba = chunk_lba + (offset / BLOCK_SIZE) as u64;
                *byte ^= 0x80 | (block_lba as u8 & 0x7f);
            }
            if let Err(error) = media.sd_write(chunk_lba, &expected) {
                return Err(alloc::format!(
                    "write failed at lba {} after {} blocks: {:?}",
                    error.failed_lba,
                    done + error.completed_blocks,
                    error.error
                ));
            }
            if let Err(error) = media.sd_flush() {
                return Err(alloc::format!(
                    "flush failed at lba {} after {} blocks: {:?}",
                    chunk_lba,
                    done + blocks,
                    error
                ));
            }
            let mut actual: alloc::vec::Vec<u8> = expected.iter().map(|byte| !byte).collect();
            if let Err(error) = media.sd_read(chunk_lba, &mut actual) {
                return Err(alloc::format!(
                    "verify read failed at lba {} after {} blocks: {:?}",
                    error.failed_lba,
                    done + error.completed_blocks,
                    error.error
                ));
            }
            if let Some(offset) = expected.iter().zip(&actual).position(|(a, b)| a != b) {
                return Err(alloc::format!(
                    "mismatch at lba {} byte {}: expected {:02x}, got {:02x} ({} blocks complete)",
                    chunk_lba + (offset / BLOCK_SIZE) as u64,
                    offset % BLOCK_SIZE,
                    expected[offset],
                    actual[offset],
                    done + blocks
                ));
            }
            done += blocks;
        }
        Ok(())
    }) else {
        println!("sdtest: no sd{}", device);
        return;
    };
    match result {
        Ok(()) => println!("sdtest: sd{} PASS lba {} count {}", device, lba, count),
        Err(error) => println!("sdtest: sd{} FAIL: {}", device, error),
    }
}

fn reset(device: usize) {
    let Some(result) = sd::with_device(device, |media| media.reset()) else {
        println!("sdreset: no sd{}", device);
        return;
    };
    match result {
        Ok(()) => println!("sdreset: sd{} reinitialized", device),
        Err(error) => println!("sdreset: sd{}: {:?}", device, error),
    }
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
