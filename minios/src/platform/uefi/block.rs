//! Block device driver for UEFI

use alloc::vec::Vec;
use core::cell::UnsafeCell;

use uefi::Identify;
use uefi::boot::ScopedProtocol;
use uefi::prelude::*;
use uefi::proto::device_path::DevicePath;
use uefi::proto::loaded_image::LoadedImage;
use uefi::proto::media::block::{BlockIO, BlockIOMedia};
use uefi::proto::media::fs::SimpleFileSystem;

use super::device_path::{
    CdromMedia, GenericDevicePathNode, HardDriveMedia, PartitionSignature, Type,
};
use super::*;

static mut BLOCK_DEVICE_MANAGER: UnsafeCell<BlockDeviceManager> =
    UnsafeCell::new(BlockDeviceManager::new());

pub struct BlockDeviceManager {
    handle_order: Vec<Handle>,
    devices: BTreeMap<Handle, UefiBlockDevice>,
    boot_device: Option<Handle>,
}

impl BlockDeviceManager {
    const fn new() -> Self {
        Self {
            handle_order: Vec::new(),
            devices: BTreeMap::new(),
            boot_device: None,
        }
    }

    #[inline]
    unsafe fn shared() -> &'static mut Self {
        unsafe { (&mut *(&raw mut BLOCK_DEVICE_MANAGER)).get_mut() }
    }

    pub unsafe fn init() {
        Self::_recognize_devices();
    }

    /// Recognizes block devices
    fn _recognize_devices() {
        unsafe {
            let shared = Self::shared();
            println!("List of Volumes:");

            let boot_device = {
                let image = get_protocol::<LoadedImage>(uefi::boot::image_handle()).unwrap();
                image.device().unwrap()
            };

            let mut handle_order = Vec::new();
            let mut devices = BTreeMap::new();

            let buffer = uefi::boot::locate_handle_buffer(uefi::boot::SearchType::ByProtocol(
                &BlockIO::GUID,
            ))
            .unwrap();
            for handle in buffer.iter().copied() {
                handle_order.push(handle);
                if let Some(device) = parse_device(handle) {
                    devices.insert(handle, device);
                }
            }

            let mut container_device_handles = Vec::new();
            for (handle, device) in &devices {
                if !device.has_partition()
                    && devices
                        .values()
                        .any(|d| d.has_partition() && d.is_sub_device_of(device))
                {
                    container_device_handles.push(*handle);
                }
            }
            for device in devices.values_mut() {
                if container_device_handles.contains(&device.handle) {
                    device.is_container_device = true;
                }
            }

            shared.boot_device = Some(boot_device);
            shared.handle_order = handle_order;
            shared.devices = devices;

            for (index, handle) in shared.handle_order.iter().enumerate() {
                let device = shared.devices.get(handle).unwrap();
                let path = get_protocol::<DevicePath>(device.handle).unwrap();
                println!("block{}: {}", index, path);
                if let Ok(fs) = get_protocol::<SimpleFileSystem>(*handle) {
                    let _ = fs;
                    println!("  * has file system");
                }
                if device.is_container_device() {
                    println!("  * container device");
                } else if let Some(partition_info) = &device.partition_info {
                    println!(
                        "  part({}) start={} size={} signature={:?}",
                        partition_info.partition_number,
                        partition_info.partition_start,
                        partition_info.partition_size,
                        partition_info.partition_signature
                    );
                }
            }
        }
    }

    pub fn drive<'a>(index: usize) -> Option<&'a mut dyn BlockDevice> {
        let shared = unsafe { Self::shared() };
        let handle = *shared.handle_order.get(index)?;
        shared
            .devices
            .get_mut(&handle)
            .map(|p| p as &mut dyn BlockDevice)
    }
}

#[allow(dead_code)]
fn parse_device(handle: Handle) -> Option<UefiBlockDevice> {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    enum State {
        #[default]
        ParsePrefix,
        ParseMedia,
    }

    let mut state = Default::default();
    unsafe {
        let path = get_protocol::<DevicePath>(handle).unwrap();
        let block_io = get_protocol::<BlockIO>(handle).unwrap();
        let media = block_io.media();

        let mut is_partitioned_device = false;
        let mut is_broken = false;
        let mut prefix_path = Vec::new();
        let mut media_path = Vec::new();
        let mut partition_info = None;

        let mut bytes = path.as_bytes();
        loop {
            let Some(node) = GenericDevicePathNode::from_bytes(bytes) else {
                is_broken = true;
                break;
            };
            if node.is_end() {
                break;
            }
            let Some((type_, _sub_type)) = node.type_() else {
                is_broken = true;
                break;
            };
            let length = node.len() as usize;

            match type_ {
                Type::Media => state = State::ParseMedia,
                _ => {}
            }

            match state {
                State::ParsePrefix => prefix_path.extend_from_slice(&bytes[..length]),
                State::ParseMedia => media_path.extend_from_slice(&bytes[..length]),
            }

            if let Some(hd) = HardDriveMedia::parse(bytes) {
                partition_info = Some(HardDrivePartition {
                    partition_number: hd.partition_number(),
                    partition_start: hd.partition_start(),
                    partition_size: hd.partition_size(),
                    partition_signature: hd.partition_signature(),
                });
                is_partitioned_device = true;
            } else if let Some(_cd) = CdromMedia::parse(bytes) {
                is_partitioned_device = true;
            }

            bytes = &bytes[length as usize..];
        }

        if is_broken {
            // device path is broken, skip this device
            return None;
        }

        let media_info = convert_media_info(media);

        let device = UefiBlockDevice {
            block_io: Box::new(block_io),
            handle,
            prefix_path,
            media_path,
            is_partitioned_device,
            is_container_device: false,
            partition_info,
            media_info,
        };

        Some(device)
    }
}

#[allow(dead_code)]
#[derive(Debug)]
pub struct UefiBlockDevice {
    /// The BlockIO protocol of the device
    block_io: Box<ScopedProtocol<BlockIO>>,
    /// The handle of the device
    handle: Handle,
    /// The part of the device path before the media node
    prefix_path: Vec<u8>,
    /// The part of the device path from the media node to the end
    media_path: Vec<u8>,
    /// Whether the device has a partition
    is_partitioned_device: bool,
    /// Whether the device is a container device
    is_container_device: bool,
    /// Information about the partition, if applicable
    partition_info: Option<HardDrivePartition>,
    /// Information about the media
    media_info: MediaInfo,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct HardDrivePartition {
    /// The partition number
    partition_number: u32,
    /// The starting LBA of the partition
    partition_start: u64,
    /// The size of the partition in blocks
    partition_size: u64,
    /// The signature of the partition
    partition_signature: PartitionSignature,
}

impl UefiBlockDevice {
    /// Returns `true` if the two devices refer to the same UEFI device.
    /// It means that they have the same handle.
    #[inline]
    pub fn is_same_device(&self, other: &UefiBlockDevice) -> bool {
        self.handle == other.handle
    }

    /// Returns `true` if the two devices refer to the same physical device.
    #[inline]
    pub fn is_same_physical_device(&self, other: &UefiBlockDevice) -> bool {
        self.prefix_path == other.prefix_path
    }

    /// Returns `true` if the device is a super-device of the other device.
    #[inline]
    pub fn is_super_device_of(&self, other: &UefiBlockDevice) -> bool {
        !self.is_same_device(other)
            && self.is_same_physical_device(other)
            && other.media_path.starts_with(&self.media_path)
    }

    /// Returns `true` if the device is a sub-device of the other device.
    #[inline]
    pub fn is_sub_device_of(&self, other: &UefiBlockDevice) -> bool {
        other.is_super_device_of(self)
    }

    /// Returns `true` if the device has a partition.
    #[inline]
    pub fn has_partition(&self) -> bool {
        self.is_partitioned_device
    }

    /// Returns `true` if the device is a container device.
    #[inline]
    pub fn is_container_device(&self) -> bool {
        self.is_container_device
    }

    /// Handles a UEFI error and converts it to a `BlockIoError`.
    fn handle_uefi_error(&mut self, err: uefi::Error<impl core::fmt::Debug>) -> BlockIoError {
        let result = match err.status() {
            Status::SUCCESS => unreachable!(),
            Status::BAD_BUFFER_SIZE => BlockIoError::BadBufferSize,
            Status::INVALID_PARAMETER => BlockIoError::InvalidParameter,
            Status::WRITE_PROTECTED => BlockIoError::WriteProtected,
            Status::NO_MEDIA => BlockIoError::NoMedia,
            Status::MEDIA_CHANGED => BlockIoError::MediaChanged,
            _ => BlockIoError::DeviceError,
        };
        if result == BlockIoError::MediaChanged {
            let info = self.block_io.media();
            self.media_info = convert_media_info(info);
        }
        result
    }
}

#[allow(dead_code)]
impl BlockDevice for UefiBlockDevice {
    fn reset(&mut self) -> Result<(), BlockIoError> {
        self.block_io
            .reset(true)
            .map_err(|e| self.handle_uefi_error(e))
    }

    fn read(&mut self, lba: LBA, buffer: &mut [u8]) -> Result<(), BlockIoError> {
        self.block_io
            .read_blocks(self.media_info.media_id.0, lba.0, buffer)
            .map_err(|e| self.handle_uefi_error(e))
    }

    fn write(&mut self, _block: LBA, _buf: &[u8]) -> Result<(), BlockIoError> {
        self.block_io
            .write_blocks(self.media_info.media_id.0, _block.0, _buf)
            .map_err(|e| self.handle_uefi_error(e))
    }

    fn media_info(&self) -> &MediaInfo {
        &self.media_info
    }
}

fn convert_media_info(info: &BlockIOMedia) -> MediaInfo {
    MediaInfo {
        media_id: MediaId(info.media_id()),
        flags: 0,
        block_size: info.block_size(),
        io_align: info.io_align(),
        block_count: LBA(info.last_block() + 1),
    }
}
