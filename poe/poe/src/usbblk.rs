//! Diagnostic commands for USB block devices (`docs/USB_MSC_RPI_PLAN.md`).
//!
//! * `usbblk` lists the devices, their state and counters.
//! * `usbread <dev> <lba> [count]` reads raw blocks and prints a CRC-32 of
//!   them and the first bytes, so a known image can be checked from outside.
//! * `usbwrite <dev> <lba>` shows that a write is refused without touching
//!   the device.
//! * `usbbench <dev> <seconds> [blocks]` reads sequentially for a while and
//!   reports the rate and the errors, for soak and throughput runs on real
//!   hardware.

use core::time::Duration;

use minios::io::fs::media::{BlockDevice, LBA};
use minios::io::usb::class::msc::{self, MediaState};
use minios::prelude::*;

/// Largest read one command takes, so a typo cannot ask for the whole disk.
const MAX_BLOCKS: u64 = 256;

pub fn command<'a>(name: &str, mut args: impl Iterator<Item = &'a str>) {
    match name {
        "usbblk" => list(),
        "usbread" => {
            let (Some(index), Some(lba)) =
                (args.next().and_then(parse), args.next().and_then(parse))
            else {
                println!("usage: usbread <dev> <lba> [count]");
                return;
            };
            let count = args.next().and_then(parse).unwrap_or(1);
            read(index as usize, lba, count);
        }
        "usbwrite" => {
            let (Some(index), Some(lba)) =
                (args.next().and_then(parse), args.next().and_then(parse))
            else {
                println!("usage: usbwrite <dev> <lba>");
                return;
            };
            write(index as usize, lba);
        }
        "usbbench" => {
            let (Some(index), Some(seconds)) =
                (args.next().and_then(parse), args.next().and_then(parse))
            else {
                println!("usage: usbbench <dev> <seconds> [blocks]");
                return;
            };
            let blocks = args.next().and_then(parse).unwrap_or(128);
            bench(index as usize, seconds, blocks);
        }
        _ => {}
    }
}

/// The counters of the device at `index`, if it is attached.
fn stats_of(index: usize) -> Option<msc::registry::DeviceStats> {
    msc::devices()
        .into_iter()
        .find(|d| d.index == index)
        .map(|d| d.stats)
}

/// Decimal, or hexadecimal with `0x`.
fn parse(text: &str) -> Option<u64> {
    match text.strip_prefix("0x") {
        Some(hex) => u64::from_str_radix(hex, 16).ok(),
        None => text.parse().ok(),
    }
}

fn list() {
    let devices = msc::devices();
    if devices.is_empty() {
        println!("usbblk: no USB block devices");
        return;
    }
    for d in devices {
        let at = match (d.info.backend, d.info.hub_port) {
            (msc::registry::Backend::Xhci, Some(hub_port)) => {
                alloc::format!("port {}.{}", d.info.port, hub_port)
            }
            (msc::registry::Backend::Xhci, None) => alloc::format!("port {}", d.info.port),
            (msc::registry::Backend::Dwc2, Some(hub_port)) => {
                alloc::format!("address {} hub port {}", d.info.port, hub_port)
            }
            (msc::registry::Backend::Dwc2, None) => alloc::format!("address {}", d.info.port),
        };
        println!(
            "usb{}: {:?} {} if {} {:04x}:{:04x} \"{}\" \"{}\" luns {}",
            d.index,
            d.info.backend,
            at,
            d.info.interface,
            d.info.vendor_id,
            d.info.product_id,
            core::str::from_utf8(&d.info.vendor)
                .unwrap_or("?")
                .trim_end(),
            core::str::from_utf8(&d.info.product)
                .unwrap_or("?")
                .trim_end(),
            d.info.max_lun as u16 + 1,
        );
        match d.state {
            MediaState::Ready {
                block_size,
                block_count,
                media_id,
            } => println!(
                "usb{}: ready {} x {} (media {})",
                d.index, block_count, block_size, media_id.0
            ),
            other => println!("usb{}: {:?}", d.index, other),
        }
        let s = d.stats;
        println!(
            "usb{}: cmds {} reads {} bytes {} errors {} retries {} transport {} timeouts {} stalls {} recoveries {} sense {:x}/{:02x}/{:02x}",
            d.index,
            s.commands,
            s.reads,
            s.bytes_read,
            s.read_errors,
            s.read_retries,
            s.transport_errors,
            s.timeouts,
            s.stalls,
            s.recoveries,
            s.last_sense.0,
            s.last_sense.1,
            s.last_sense.2,
        );
    }
}

fn read(index: usize, lba: u64, count: u64) {
    if count == 0 || count > MAX_BLOCKS {
        println!("usbread: count must be 1..={}", MAX_BLOCKS);
        return;
    }
    let mut device = match msc::open(index) {
        Ok(device) => device,
        Err(error) => {
            println!("usbread: usb{}: {:?}", index, error);
            return;
        }
    };
    let block_size = device.media_info().block_size as u64;
    let mut buffer = Vec::new();
    buffer.resize((block_size * count) as usize, 0u8);
    match device.read(LBA(lba), &mut buffer) {
        Ok(()) => {
            println!(
                "usbread: usb{} lba {} count {} block_size {} bytes {} crc32 {:08x}",
                index,
                lba,
                count,
                block_size,
                buffer.len(),
                crc32(&buffer)
            );
            let mut line = String::new();
            for byte in &buffer[..16] {
                let _ = write!(line, " {:02x}", byte);
            }
            println!("usbread: usb{} lba {}:{}", index, lba, line);
        }
        Err(error) => println!(
            "usbread: usb{} lba {} count {}: {:?}",
            index, lba, count, error
        ),
    }
}

fn write(index: usize, lba: u64) {
    let mut device = match msc::open(index) {
        Ok(device) => device,
        Err(error) => {
            println!("usbwrite: usb{}: {:?}", index, error);
            return;
        }
    };
    let buffer = vec_of(device.media_info().block_size as usize);
    println!(
        "usbwrite: usb{} lba {}: {:?}",
        index,
        lba,
        device.write(LBA(lba), &buffer)
    );
}

fn bench(index: usize, seconds: u64, blocks: u64) {
    if blocks == 0 || blocks > MAX_BLOCKS || seconds == 0 {
        println!(
            "usbbench: blocks must be 1..={} and seconds at least 1",
            MAX_BLOCKS
        );
        return;
    }
    let mut device = match msc::open(index) {
        Ok(device) => device,
        Err(error) => {
            println!("usbbench: usb{}: {:?}", index, error);
            return;
        }
    };
    let info = *device.media_info();
    let blocks = blocks.min(info.block_count.0);
    let buffer_len = (info.block_size as u64 * blocks) as usize;
    let mut buffer = vec_of(buffer_len);
    let mut timer = Event::with_timeout(Duration::from_secs(seconds));
    // The rate is worked out from the hardware counter, not from `seconds`:
    // the timer above counts interrupt ticks, which run slow whenever the
    // interrupt is serviced late, so the loop can run well past `seconds`.
    let started = msc::now_us();
    let before = stats_of(index);
    let (mut lba, mut reads, mut errors, mut bytes) = (0u64, 0u64, 0u64, 0u64);
    let mut last_error = None;
    let mut stopped_early = false;
    while matches!(timer.poll(), PollResult::Pending) {
        if lba + blocks > info.block_count.0 {
            lba = 0;
        }
        match device.read(LBA(lba), &mut buffer) {
            Ok(()) => {
                reads += 1;
                bytes += buffer_len as u64;
            }
            Err(error) => {
                errors += 1;
                last_error = Some(error);
                // A handle whose device or medium has gone will not work
                // again; stop rather than spin on it.
                if matches!(
                    error,
                    minios::io::fs::media::BlockIoError::NoMedia
                        | minios::io::fs::media::BlockIoError::MediaChanged
                ) {
                    stopped_early = true;
                    break;
                }
            }
        }
        lba += blocks;
    }
    let elapsed_us = msc::now_us().wrapping_sub(started);
    minios::System::poll_services();
    let after = stats_of(index);
    let rate = if stopped_early {
        String::from("stopped early")
    } else if elapsed_us == 0 {
        String::from("no clock")
    } else {
        alloc::format!(
            "{} KiB/s over {}.{:03} s",
            bytes as u128 * 1_000_000 / 1024 / elapsed_us as u128,
            elapsed_us / 1_000_000,
            elapsed_us / 1000 % 1000
        )
    };
    println!(
        "usbbench: usb{} {} reads of {} blocks, {} KiB ({}), {} errors{}",
        index,
        reads,
        blocks,
        bytes / 1024,
        rate,
        errors,
        match last_error {
            Some(error) => alloc::format!(", last {:?}", error),
            None => String::new(),
        }
    );
    // What the bus itself delivered, for comparing with the count above: the
    // controller's received byte counts, the SCSI commands behind them, and a
    // CRC of the last buffer to show it holds data (compare with usbread).
    if let (Some(before), Some(after)) = (before, after) {
        let commands = after.commands.wrapping_sub(before.commands);
        let bus = after.bus_bytes_in.wrapping_sub(before.bus_bytes_in);
        println!(
            "usbbench: bus received {} KiB in {} commands ({} KiB each), last buffer lba {} crc32 {:08x}",
            bus / 1024,
            commands,
            if commands == 0 {
                0
            } else {
                bus / 1024 / commands
            },
            lba.wrapping_sub(blocks),
            crc32(&buffer)
        );
    }
}

fn vec_of(len: usize) -> Vec<u8> {
    let mut v = Vec::new();
    v.resize(len, 0);
    v
}

/// CRC-32 (IEEE 802.3, the one `zlib.crc32` computes).
fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &byte in bytes {
        crc ^= byte as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xedb8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}
