//! Single queue VirtIO block device adapter.
use alloc::boxed::Box;

use super::Device;
use super::dma::Dma;
use super::queue::Buffer;
use crate::System;
use crate::io::fs::media::{BlockDevice, BlockIoError, LBA, MediaId, MediaInfo};

const RO: u64 = 1 << 5;
const BLK_SIZE: u64 = 1 << 6;
const FLUSH: u64 = 1 << 9;
const MAX_TRANSFER: usize = 128 * 1024;

pub struct VirtioBlock {
    device: Device,
    media: MediaInfo,
    read_only: bool,
    flush: bool,
}

static mut DEVICES: [Option<&'static mut VirtioBlock>; 8] = [const { None }; 8];

pub(super) fn attach(base: usize, size: usize) -> Result<(), &'static str> {
    let slot = unsafe {
        (&raw mut DEVICES)
            .as_mut()
            .unwrap()
            .iter_mut()
            .find(|entry| entry.is_none())
    }
    .ok_or("too many virtio disks")?;
    let device = unsafe { Device::new(base, size, RO | BLK_SIZE | FLUSH) }?;
    if device.queue.capacity() < 3 {
        return Err("block queue has fewer than three descriptors");
    }
    let config_limit = super::deadline(1_000_000);
    let (capacity, block_size) = loop {
        let generation = device.transport.config_generation();
        let low = device.transport.config_u32(0) as u64;
        let high = device.transport.config_u32(4) as u64;
        let block_size = if device.features & BLK_SIZE != 0 {
            device.transport.config_u32(20)
        } else {
            512
        };
        if generation == device.transport.config_generation() {
            break (low | high << 32, block_size);
        }
        if super::expired(config_limit) {
            return Err("block config changed continuously");
        }
    };
    if block_size < 512
        || !block_size.is_multiple_of(512)
        || !block_size.is_power_of_two()
        || block_size > 4096
    {
        return Err("invalid logical block size");
    }
    let block_count = capacity / (block_size as u64 / 512);
    if block_count == 0 {
        return Err("empty disk");
    }
    let disk = Box::leak(Box::new(VirtioBlock {
        read_only: device.features & RO != 0,
        flush: device.features & FLUSH != 0,
        media: MediaInfo {
            media_id: MediaId::ZERO,
            flags: 0,
            block_size,
            io_align: 1,
            block_count: LBA(block_count),
        },
        device,
    }));
    crate::println!(
        "virtio-blk: {} blocks of {} bytes{}",
        block_count,
        block_size,
        if disk.read_only { " (read-only)" } else { "" }
    );
    *slot = Some(disk);
    Ok(())
}

pub fn count() -> usize {
    unsafe {
        (&raw const DEVICES)
            .as_ref()
            .unwrap()
            .iter()
            .filter(|entry| entry.is_some())
            .count()
    }
}

/// # Safety
/// The caller must serialize access to each disk and must not hold two
/// references to the same disk at once.
pub unsafe fn device(index: usize) -> Option<&'static mut VirtioBlock> {
    unsafe { (&raw mut DEVICES).as_mut()?.get_mut(index)?.as_deref_mut() }
}

impl VirtioBlock {
    fn command(
        &mut self,
        operation: u32,
        sector: u64,
        data: Option<&mut [u8]>,
    ) -> Result<(), BlockIoError> {
        if self.device.config_changed() {
            return Err(BlockIoError::DeviceError);
        }
        let data_len = data.as_ref().map_or(0, |d| d.len());
        let mut header = Dma::new(16, 16)
            .map_err(|_| BlockIoError::DeviceError)?
            .with_owner(self.device.owner.clone());
        header.bytes_mut()[0..4].copy_from_slice(&operation.to_le_bytes());
        header.bytes_mut()[8..16].copy_from_slice(&sector.to_le_bytes());
        let payload = if let Some(buffer) = data.as_ref() {
            let mut dma = Dma::new(buffer.len(), 16)
                .map_err(|_| BlockIoError::DeviceError)?
                .with_owner(self.device.owner.clone());
            if operation == 1 {
                dma.bytes_mut()[..buffer.len()].copy_from_slice(buffer);
            }
            Some(dma)
        } else {
            None
        };
        let mut status = Dma::new(1, 16)
            .map_err(|_| BlockIoError::DeviceError)?
            .with_owner(self.device.owner.clone());
        status.bytes_mut()[0] = 0xff;
        header.release();
        if let Some(payload) = payload.as_ref() {
            payload.release();
        }
        status.release();
        let mut buffers = [Buffer::default(); 3];
        buffers[0] = Buffer {
            address: header.addr(),
            length: 16,
            writable: false,
        };
        let mut count = 1;
        if let Some(payload) = payload.as_ref() {
            buffers[count] = Buffer {
                address: payload.addr(),
                length: data_len as u32,
                writable: operation == 0,
            };
            count += 1;
        }
        buffers[count] = Buffer {
            address: status.addr(),
            length: 1,
            writable: true,
        };
        count += 1;
        let used = self
            .device
            .run(&buffers[..count], 5_000_000)
            .map_err(|_| BlockIoError::DeviceError)?;
        status.acquire();
        if status.bytes()[0] != 0 || (operation != 0 && used != 1) {
            return Err(BlockIoError::DeviceError);
        }
        if operation == 0 {
            if used != data_len as u32 + 1 {
                return Err(BlockIoError::DeviceError);
            }
            if let (Some(payload), Some(target)) = (payload.as_ref(), data) {
                payload.acquire();
                target.copy_from_slice(&payload.bytes()[..data_len]);
            }
        }
        Ok(())
    }
    fn validate(&self, lba: LBA, len: usize) -> Result<u64, BlockIoError> {
        let block_size = self.media.block_size as usize;
        if len == 0 || !len.is_multiple_of(block_size) {
            return Err(BlockIoError::BadBufferSize);
        }
        let blocks = (len / block_size) as u64;
        if lba
            .0
            .checked_add(blocks)
            .is_none_or(|end| end > self.media.block_count.0)
        {
            return Err(BlockIoError::InvalidParameter);
        }
        lba.0
            .checked_mul(self.media.block_size as u64 / 512)
            .ok_or(BlockIoError::InvalidParameter)
    }
    fn transfer(&mut self, lba: LBA, buffer: &mut [u8]) -> Result<(), BlockIoError> {
        let mut sector = self.validate(lba, buffer.len())?;
        let chunk = MAX_TRANSFER / self.media.block_size as usize * self.media.block_size as usize;
        for part in buffer.chunks_mut(chunk) {
            self.command(0, sector, Some(part))?;
            sector += part.len() as u64 / 512;
        }
        Ok(())
    }
}

impl BlockDevice for VirtioBlock {
    fn reset(&mut self) -> Result<(), BlockIoError> {
        self.device
            .restart(RO | BLK_SIZE | FLUSH)
            .map_err(|_| BlockIoError::DeviceError)?;
        let config_limit = super::deadline(1_000_000);
        let (capacity, block_size) = loop {
            let generation = self.device.transport.config_generation();
            let low = self.device.transport.config_u32(0) as u64;
            let high = self.device.transport.config_u32(4) as u64;
            let size = if self.device.features & BLK_SIZE != 0 {
                self.device.transport.config_u32(20)
            } else {
                512
            };
            if generation == self.device.transport.config_generation() {
                break (low | high << 32, size);
            }
            if super::expired(config_limit) {
                return Err(BlockIoError::DeviceError);
            }
        };
        if block_size < 512
            || !block_size.is_multiple_of(512)
            || !block_size.is_power_of_two()
            || block_size > 4096
        {
            return Err(BlockIoError::DeviceError);
        }
        let count = capacity / (block_size as u64 / 512);
        if count == 0 {
            return Err(BlockIoError::NoMedia);
        }
        self.media.media_id.succ();
        self.media.block_size = block_size;
        self.media.block_count = LBA(count);
        self.read_only = self.device.features & RO != 0;
        self.flush = self.device.features & FLUSH != 0;
        Ok(())
    }
    fn read(&mut self, lba: LBA, buffer: &mut [u8]) -> Result<(), BlockIoError> {
        self.transfer(lba, buffer)
    }
    fn write(&mut self, lba: LBA, buffer: &[u8]) -> Result<(), BlockIoError> {
        if self.read_only {
            return Err(BlockIoError::WriteProtected);
        }
        let mut sector = self.validate(lba, buffer.len())?;
        let chunk = MAX_TRANSFER / self.media.block_size as usize * self.media.block_size as usize;
        for part in buffer.chunks(chunk) {
            let mut copy = part.to_vec();
            self.command(1, sector, Some(&mut copy))?;
            sector += part.len() as u64 / 512;
        }
        Ok(())
    }
    fn flush(&mut self) -> Result<(), BlockIoError> {
        if self.read_only {
            return Ok(());
        }
        if !self.flush {
            return Err(BlockIoError::DeviceError);
        }
        self.command(4, 0, None)
    }
    fn media_info(&mut self) -> &MediaInfo {
        &self.media
    }
}

#[cfg(test)]
mod tests {
    use alloc::sync::Arc;
    use std::sync::Mutex;

    use super::*;
    use crate::io::virtio::transport::Transport;

    #[derive(Default)]
    struct State {
        status: u32,
        desc: u64,
        used: u64,
        used_index: u16,
        used_len: u32,
        command_status: u8,
    }

    struct Fake(Arc<Mutex<State>>);
    impl Transport for Fake {
        fn status(&self) -> u32 {
            self.0.lock().unwrap().status
        }
        fn set_status(&self, value: u32) {
            self.0.lock().unwrap().status = value;
        }
        fn features(&self) -> u64 {
            1 << 32
        }
        fn set_features(&self, _: u64) {}
        fn select_queue(&self, _: u16) {}
        fn queue_max(&self) -> u32 {
            8
        }
        fn queue_ready(&self) -> bool {
            false
        }
        fn setup_queue(&self, _: u16, desc: u64, _: u64, used: u64) {
            let mut state = self.0.lock().unwrap();
            state.desc = desc;
            state.used = used;
            state.used_index = 0;
        }
        fn notify(&self, _: u16) {
            let mut state = self.0.lock().unwrap();
            let desc = state.desc as *const u8;
            let header = u64::from_le(unsafe { (desc as *const u64).read_unaligned() });
            let operation = u32::from_le(unsafe { (header as *const u32).read_unaligned() });
            let status_desc = if operation == 4 { 1 } else { 2 };
            let status_addr = u64::from_le(unsafe {
                (desc.add(status_desc * 16) as *const u64).read_unaligned()
            });
            unsafe { (status_addr as *mut u8).write(state.command_status) };
            let slot = state.used_index as usize % 8;
            unsafe {
                ((state.used as usize + 4 + slot * 8) as *mut u32).write_unaligned(0);
                ((state.used as usize + 8 + slot * 8) as *mut u32)
                    .write_unaligned(state.used_len.to_le());
                state.used_index = state.used_index.wrapping_add(1);
                ((state.used as usize + 2) as *mut u16).write_unaligned(state.used_index.to_le());
            }
        }
        fn ack_interrupt(&self) -> u32 {
            0
        }
        fn config_u32(&self, _: usize) -> u32 {
            0
        }
        fn config_generation(&self) -> u32 {
            0
        }
    }

    #[test]
    fn rejects_short_read_long_write_and_device_error() {
        let state = Arc::new(Mutex::new(State::default()));
        let device = Device::from_transport(Box::new(Fake(state.clone())), 0).unwrap();
        let mut disk = VirtioBlock {
            device,
            media: MediaInfo {
                media_id: MediaId::ZERO,
                flags: 0,
                block_size: 512,
                io_align: 1,
                block_count: LBA(8),
            },
            read_only: false,
            flush: true,
        };
        let mut data = [0; 512];
        state.lock().unwrap().used_len = 1;
        assert!(matches!(
            disk.command(0, 0, Some(&mut data)),
            Err(BlockIoError::DeviceError)
        ));
        state.lock().unwrap().used_len = 2;
        assert!(matches!(
            disk.command(1, 0, Some(&mut data)),
            Err(BlockIoError::DeviceError)
        ));
        {
            let mut state = state.lock().unwrap();
            state.used_len = 1;
            state.command_status = 1;
        }
        assert!(matches!(
            disk.command(4, 0, None),
            Err(BlockIoError::DeviceError)
        ));
    }
}
