//! syscon device for virt machine

#[repr(u32)]
#[derive(Debug, Clone, Copy)]
pub enum Syscon {
    PowerOff = 0x5555,
    Reboot = 0x7777,
}

impl Syscon {
    #[inline]
    pub fn write(&self) {
        unsafe {
            let p = 0x0010_0000 as *mut u32;
            p.write_volatile(*self as u32);
        }
    }
}
