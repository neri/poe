//! Operations shared by VirtIO transports. Register offsets are private to
//! each transport implementation.
pub trait Transport: Send {
    fn status(&self) -> u32;
    fn set_status(&self, value: u32);
    fn features(&self) -> u64;
    fn set_features(&self, value: u64);
    fn select_queue(&self, index: u16);
    fn queue_max(&self) -> u32;
    fn queue_ready(&self) -> bool;
    fn setup_queue(&self, size: u16, desc: u64, avail: u64, used: u64);
    fn notify(&self, index: u16);
    fn ack_interrupt(&self) -> u32;
    fn config_u32(&self, offset: usize) -> u32;
    fn config_generation(&self) -> u32;
}
