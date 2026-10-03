//! SD card protocol and block-device adapter.

use alloc::boxed::Box;
use alloc::vec::Vec;

use sd::Card;

pub mod dw_mmc;
pub mod sdhci;
pub use sd::{
    Addressing, BusWidth, CardError, CardInfo, CardKind, DataDirection, Host, HostError, Response,
    ResponseType, TransferError,
};

use crate::io::fs::media::{BlockDevice, BlockIoError, LBA, MediaId, MediaInfo};

/// Public SD-specific registry view, including information omitted by the
/// generic BlockDevice trait.
pub trait SdMedia: BlockDevice {
    fn card_info(&self) -> Option<CardInfo>;
    fn sd_read(&mut self, lba: u64, buffer: &mut [u8]) -> Result<(), TransferError>;
    fn sd_write(&mut self, lba: u64, buffer: &[u8]) -> Result<(), TransferError>;
    fn sd_flush(&mut self) -> Result<(), CardError>;
}

impl<H: Host> SdMedia for SdBlockDevice<H> {
    fn card_info(&self) -> Option<CardInfo> {
        self.card()
            .filter(|card| card.is_usable())
            .map(|card| *card.info())
    }

    fn sd_read(&mut self, lba: u64, buffer: &mut [u8]) -> Result<(), TransferError> {
        self.card
            .as_mut()
            .ok_or_else(|| sd::TransferError {
                error: CardError::NeedsReinitialization,
                failed_lba: lba,
                completed_blocks: 0,
            })?
            .read_detailed(lba, buffer)
    }

    fn sd_write(&mut self, lba: u64, buffer: &[u8]) -> Result<(), TransferError> {
        self.card
            .as_mut()
            .ok_or_else(|| sd::TransferError {
                error: CardError::NeedsReinitialization,
                failed_lba: lba,
                completed_blocks: 0,
            })?
            .write_detailed(lba, buffer)
    }

    fn sd_flush(&mut self) -> Result<(), CardError> {
        self.card
            .as_mut()
            .ok_or(CardError::NeedsReinitialization)?
            .flush()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeviceSummary {
    pub index: usize,
    pub info: Option<CardInfo>,
}

static mut DEVICES: Vec<Box<dyn SdMedia>> = Vec::new();

/// Initialize and publish one SD host. Failed cards are never visible as
/// block devices.
pub fn register<H: Host + 'static>(host: H) -> Result<usize, CardError> {
    let device = SdBlockDevice::initialize(host)?;
    let devices = unsafe { &mut *(&raw mut DEVICES) };
    let index = devices.len();
    devices.push(Box::new(device));
    Ok(index)
}

pub fn devices() -> Vec<DeviceSummary> {
    let devices = unsafe { &*(&raw const DEVICES) };
    devices
        .iter()
        .enumerate()
        .map(|(index, device)| DeviceSummary {
            index,
            info: device.card_info(),
        })
        .collect()
}

/// Runs a synchronous operation on an attached card. SD access is serialized
/// by the single foreground caller, just like the USB MSC registry.
pub fn with_device<R>(index: usize, f: impl FnOnce(&mut dyn SdMedia) -> R) -> Option<R> {
    let devices = unsafe { &mut *(&raw mut DEVICES) };
    devices.get_mut(index).map(|device| f(device.as_mut()))
}

/// BlockDevice view over an initialized SD card. Detailed SD errors remain
/// available through [`SdBlockDevice::card`] and the POE diagnostic API.
pub struct SdBlockDevice<H: Host> {
    card: Option<Card<H>>,
    info: MediaInfo,
}

impl<H: Host> SdBlockDevice<H> {
    pub fn initialize(host: H) -> Result<Self, CardError> {
        let card = Card::initialize(host)?;
        let info = media_info(card.info().capacity_blocks);
        Ok(Self {
            card: Some(card),
            info,
        })
    }

    pub fn card(&self) -> Option<&Card<H>> {
        self.card.as_ref()
    }

    pub fn card_mut(&mut self) -> Option<&mut Card<H>> {
        self.card.as_mut()
    }
}

impl<H: Host> BlockDevice for SdBlockDevice<H> {
    fn reset(&mut self) -> Result<(), BlockIoError> {
        let card = self.card.as_mut().ok_or(BlockIoError::DeviceError)?;
        card.reinitialize().map_err(map_error)?;
        self.info = media_info(card.info().capacity_blocks);
        self.info.media_id.succ();
        Ok(())
    }

    fn read(&mut self, lba: LBA, buffer: &mut [u8]) -> Result<(), BlockIoError> {
        self.sd_read(lba.0, buffer)
            .map_err(|failure| map_error(failure.error))
    }

    fn write(&mut self, lba: LBA, buffer: &[u8]) -> Result<(), BlockIoError> {
        self.sd_write(lba.0, buffer)
            .map_err(|failure| map_error(failure.error))
    }

    fn flush(&mut self) -> Result<(), BlockIoError> {
        self.sd_flush().map_err(map_error)
    }

    fn media_info(&mut self) -> &MediaInfo {
        &self.info
    }
}

const fn media_info(block_count: u64) -> MediaInfo {
    MediaInfo {
        media_id: MediaId::ZERO,
        flags: 0,
        block_size: sd::BLOCK_SIZE as u32,
        io_align: 1,
        block_count: LBA(block_count),
    }
}

fn map_error(error: CardError) -> BlockIoError {
    match error {
        CardError::InvalidBuffer => BlockIoError::BadBufferSize,
        CardError::OutOfRange => BlockIoError::InvalidParameter,
        CardError::Host(HostError::Removed) => BlockIoError::NoMedia,
        _ => BlockIoError::DeviceError,
    }
}
