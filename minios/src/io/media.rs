pub trait BlockDevice {
    fn reset(&mut self) -> Result<(), BlockIoError>;

    fn read(&mut self, block: LBA, buf: &mut [u8]) -> Result<(), BlockIoError>;

    fn write(&mut self, block: LBA, buf: &[u8]) -> Result<(), BlockIoError>;

    fn media_info(&self) -> MediaInfo;
}

#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct LBA(pub u64);

#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct MediaId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockIoError {
    DeviceError,
    InvalidParameter,
    WriteProtected,
    NoMedia,
    MediaChanged,
}

#[derive(Debug, Clone, Copy)]
pub struct MediaInfo {
    pub media_id: MediaId,
    pub flags: u32,
    pub block_size: u32,
    pub io_align: u32,
    pub block_count: LBA,
}
