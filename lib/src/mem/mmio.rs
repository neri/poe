pub unsafe trait Mmio32 {
    fn addr(&self) -> usize;

    #[inline]
    unsafe fn write(&self, val: u32) {
        unsafe {
            let p = self.addr() as *mut u32;
            p.write_volatile(val);
        }
    }

    #[inline]
    unsafe fn read(&self) -> u32 {
        unsafe {
            let p = self.addr() as *const u32;
            p.read_volatile()
        }
    }
}

pub unsafe trait Mmio64 {
    fn addr(&self) -> usize;

    #[inline]
    unsafe fn write(&self, val: u64) {
        unsafe {
            let p = self.addr() as *mut u64;
            p.write_volatile(val);
        }
    }

    #[inline]
    unsafe fn read(&self) -> u64 {
        unsafe {
            let p = self.addr() as *const u64;
            p.read_volatile()
        }
    }
}

#[repr(transparent)]
pub struct Mmio32Reg(pub usize);

unsafe impl Mmio32 for Mmio32Reg {
    #[inline]
    fn addr(&self) -> usize {
        self.0
    }
}

#[repr(transparent)]
pub struct Mmio64Reg(pub usize);

unsafe impl Mmio64 for Mmio64Reg {
    #[inline]
    fn addr(&self) -> usize {
        self.0
    }
}
