//! Block Device definitions.

pub trait BlockDevice {
    /// Resets the device
    fn reset(&mut self) -> Result<(), BlockIoError>;

    /// Reads blocks from the device
    fn read(&mut self, block: LBA, buf: &mut [u8]) -> Result<(), BlockIoError>;

    /// Writes blocks to the device
    fn write(&mut self, block: LBA, buf: &[u8]) -> Result<(), BlockIoError>;

    /// Flushes any buffered data to the device
    fn flush(&mut self) -> Result<(), BlockIoError> {
        Ok(())
    }

    /// Returns information about the media
    fn media_info(&self) -> &MediaInfo;
}

#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct LBA(pub u64);

#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct MediaId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockIoError {
    /// A generic error occurred
    DeviceError,
    /// The specified parameter is invalid
    InvalidParameter,
    /// The device is write-protected
    WriteProtected,
    /// No media is present
    NoMedia,
    /// The media has changed
    MediaChanged,
    /// The buffer size is not a multiple of the block size of the device
    BadBufferSize,
}

#[derive(Debug, Clone, Copy)]
pub struct MediaInfo {
    /// The media ID, which changes when the media is changed.
    pub media_id: MediaId,
    /// TBD
    pub flags: u32,
    /// The size of a block in bytes.
    pub block_size: u32,
    pub io_align: u32,
    /// The total number of blocks on the media.
    pub block_count: LBA,
}
