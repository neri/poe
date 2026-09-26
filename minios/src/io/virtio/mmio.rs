//! VirtIO 1.x MMIO transport. Register offsets are contained here.
use super::barrier;
use super::transport::Transport;

#[derive(Clone, Copy)]
pub struct Mmio {
    base: usize,
}

impl Transport for Mmio {
    fn status(&self) -> u32 {
        Mmio::status(self)
    }
    fn set_status(&self, value: u32) {
        Mmio::set_status(self, value)
    }
    fn features(&self) -> u64 {
        Mmio::features(self)
    }
    fn set_features(&self, value: u64) {
        Mmio::set_features(self, value)
    }
    fn select_queue(&self, index: u16) {
        Mmio::select_queue(self, index)
    }
    fn queue_max(&self) -> u32 {
        Mmio::queue_max(self)
    }
    fn queue_ready(&self) -> bool {
        Mmio::queue_ready(self)
    }
    fn setup_queue(&self, size: u16, desc: u64, avail: u64, used: u64) {
        Mmio::setup_queue(self, size, desc, avail, used)
    }
    fn notify(&self, index: u16) {
        Mmio::notify(self, index)
    }
    fn ack_interrupt(&self) -> u32 {
        Mmio::ack_interrupt(self)
    }
    fn config_u32(&self, offset: usize) -> u32 {
        Mmio::config_u32(self, offset)
    }
    fn config_generation(&self) -> u32 {
        Mmio::config_generation(self)
    }
}

impl Mmio {
    /// Caller guarantees that `base..base+0x200` is mapped device memory.
    pub unsafe fn new(base: usize, size: usize) -> Result<Self, &'static str> {
        if base & 3 != 0 || size < 0x200 || base.checked_add(size).is_none() {
            return Err("invalid virtio MMIO range");
        }
        let this = Self { base };
        if this.read(0x000) != 0x7472_6976 || this.read(0x004) != 2 {
            return Err("not modern virtio MMIO");
        }
        Ok(this)
    }
    pub fn read(&self, offset: usize) -> u32 {
        barrier::device();
        unsafe { ((self.base + offset) as *const u32).read_volatile() }
    }
    pub fn write(&self, offset: usize, value: u32) {
        unsafe { ((self.base + offset) as *mut u32).write_volatile(value) };
        barrier::device();
    }
    pub fn device_id(&self) -> u32 {
        self.read(0x008)
    }
    pub fn status(&self) -> u32 {
        self.read(0x070)
    }
    pub fn set_status(&self, value: u32) {
        self.write(0x070, value)
    }
    pub fn features(&self) -> u64 {
        self.write(0x014, 0);
        let low = self.read(0x010) as u64;
        self.write(0x014, 1);
        low | ((self.read(0x010) as u64) << 32)
    }
    pub fn set_features(&self, value: u64) {
        self.write(0x024, 0);
        self.write(0x020, value as u32);
        self.write(0x024, 1);
        self.write(0x020, (value >> 32) as u32);
    }
    pub fn select_queue(&self, index: u16) {
        self.write(0x030, index as u32)
    }
    pub fn queue_max(&self) -> u32 {
        self.read(0x034)
    }
    pub fn queue_ready(&self) -> bool {
        self.read(0x044) != 0
    }
    pub fn setup_queue(&self, size: u16, desc: u64, avail: u64, used: u64) {
        self.write(0x038, size as u32);
        for (low, address) in [(0x080, desc), (0x090, avail), (0x0a0, used)] {
            self.write(low, address as u32);
            self.write(low + 4, (address >> 32) as u32);
        }
        self.write(0x044, 1);
    }
    pub fn notify(&self, index: u16) {
        self.write(0x050, index as u32)
    }
    pub fn ack_interrupt(&self) -> u32 {
        let status = self.read(0x060);
        if status != 0 {
            self.write(0x064, status);
        }
        status
    }
    pub fn config_u32(&self, offset: usize) -> u32 {
        self.read(0x100 + offset)
    }
    pub fn config_generation(&self) -> u32 {
        self.read(0x0fc)
    }
}
