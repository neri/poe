pub const GOTGCTL: usize = 0x000;
pub const GAHBCFG: usize = 0x008;
pub const GUSBCFG: usize = 0x00c;
pub const GRSTCTL: usize = 0x010;
pub const GINTSTS: usize = 0x014;
pub const GINTMSK: usize = 0x018;
pub const GRXFSIZ: usize = 0x024;
pub const GNPTXFSIZ: usize = 0x028;
pub const GSNPSID: usize = 0x040;
pub const GHWCFG1: usize = 0x044;
pub const GHWCFG2: usize = 0x048;
pub const GHWCFG3: usize = 0x04c;
pub const GHWCFG4: usize = 0x050;
pub const HPTXFSIZ: usize = 0x100;
pub const HCFG: usize = 0x400;
pub const HFNUM: usize = 0x408;
pub const HAINT: usize = 0x414;
pub const HAINTMSK: usize = 0x418;
pub const HPRT: usize = 0x440;
pub const PCGCTL: usize = 0xe00;
pub const HC_BASE: usize = 0x500;
pub const HC_STRIDE: usize = 0x20;
pub const HCCHAR: usize = 0x00;
pub const HCSPLT: usize = 0x04;
pub const HCINT: usize = 0x08;
pub const HCINTMSK: usize = 0x0c;
pub const HCTSIZ: usize = 0x10;
pub const HCDMA: usize = 0x14;

#[inline]
pub unsafe fn read(base: usize, offset: usize) -> u32 {
    unsafe { ((base + offset) as *const u32).read_volatile() }
}
#[inline]
pub unsafe fn write(base: usize, offset: usize, value: u32) {
    unsafe { ((base + offset) as *mut u32).write_volatile(value) }
}
#[inline]
pub const fn channel(channel: usize, reg: usize) -> usize {
    HC_BASE + channel * HC_STRIDE + reg
}
