use crate::{mem::mmio::Mmio32, *};
use core::{
    arch::asm,
    marker::PhantomData,
    sync::atomic::{Ordering, compiler_fence},
};

#[allow(dead_code)]
#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Copy)]
pub enum Mbox {
    POWER = 0,
    FB = 1,
    VUART = 2,
    VCHIQ = 3,
    LEDS = 4,
    BTNS = 5,
    TOUCH = 6,
    COUNT = 7,
    PROP = 8,
}

impl Mbox {
    #[inline]
    pub const fn new<const N: usize>(&self) -> MboxContext<Request, N> {
        MboxContext::new(*self)
    }
}

pub struct MboxContext<CONTEXT: MboxContextType, const N: usize> {
    payload: Payload<N>,
    chan: Mbox,
    index: usize,
    _phantom: PhantomData<CONTEXT>,
}

impl<CONTEXT: MboxContextType, const N: usize> MboxContext<CONTEXT, N> {
    const REQUEST: u32 = 0x0000_0000;
    const RESPONSE: u32 = 0x8000_0000;
    const FULL: u32 = 0x8000_0000;
    const EMPTY: u32 = 0x4000_0000;

    pub fn mbox_addr(&self) -> u32 {
        let p = self.payload.0.as_ptr() as usize as u32;
        p | (self.chan as u32)
    }
}

impl<const N: usize> MboxContext<Request, N> {
    #[inline]
    pub const fn new(chan: Mbox) -> Self {
        assert!(N >= 6);
        let mut mbox = Self {
            payload: Payload([0; N]),
            chan,
            index: 2,
            _phantom: PhantomData,
        };
        mbox.payload.0[1] = Self::REQUEST;
        mbox
    }

    #[inline]
    pub fn append(&mut self, tag: Tag) -> Result<usize, ()> {
        tag.append_to(self)
    }

    #[inline]
    fn _push(&mut self, val: u32) -> Result<usize, ()> {
        match self.payload.0.get_mut(self.index as usize) {
            Some(p) => {
                *p = val;
                self.index += 1;
                Ok(self.index)
            }
            None => Err(()),
        }
    }

    #[inline]
    fn _push_slice(&mut self, data: &[u32]) -> Result<usize, ()> {
        for val in data {
            self._push(*val)?;
        }
        Ok(self.index)
    }

    #[inline]
    fn _push_dummy(&mut self, len: usize) -> Result<usize, ()> {
        for _ in 0..len {
            self._push(0)?;
        }
        Ok(self.index)
    }

    #[inline]
    unsafe fn flush_payload(&self) {
        compiler_fence(Ordering::SeqCst);
        unsafe {
            asm!("dc civac, {}", in(reg) self.payload.0.as_ptr());
        }
    }

    pub fn call(mut self) -> Result<MboxContext<Response, N>, ()> {
        unsafe {
            self._push(RawTag::End.as_u32())?;
            self.payload.0[0] = self.index as u32 * 4;

            self.flush_payload();

            let mbox_addr = self.mbox_addr();

            while (Regs::STATUS.read() & Self::FULL) != 0 {
                Hal::cpu().no_op();
            }

            compiler_fence(Ordering::SeqCst);
            Regs::WRITE.write(mbox_addr);
            compiler_fence(Ordering::SeqCst);

            loop {
                while (Regs::STATUS.read() & Self::EMPTY) != 0 {
                    Hal::cpu().no_op();
                }

                if Regs::READ.read() == mbox_addr {
                    break;
                }
            }

            self.flush_payload();

            if self.payload.0[1] == Self::RESPONSE {
                Ok(MboxContext {
                    payload: self.payload,
                    chan: self.chan,
                    index: self.index,
                    _phantom: PhantomData,
                })
            } else {
                Err(())
            }
        }
    }
}

impl<const N: usize> MboxContext<Response, N> {
    #[inline]
    #[track_caller]
    pub fn slice(&self) -> &[u32] {
        &self.payload.0[..self.index]
    }

    #[inline]
    #[track_caller]
    pub fn response(&self, index: usize) -> u32 {
        self.slice()[index]
    }

    #[inline]
    #[track_caller]
    pub fn response_slice<const M: usize>(&self, index: usize) -> &[u32; M] {
        self.slice()[index..index + M].try_into().unwrap()
    }
}

pub trait MboxContextType {}

pub struct Request;

impl MboxContextType for Request {}

pub struct Response;

impl MboxContextType for Response {}

#[allow(dead_code)]
#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Copy)]
enum Regs {
    READ = 0x00,
    POLL = 0x10,
    SENDER = 0x14,
    STATUS = 0x18,
    CONFIG = 0x1C,
    WRITE = 0x20,
}

impl Regs {
    #[inline]
    pub fn base_addr() -> usize {
        super::mmio_base() + 0x0000_B880
    }
}

unsafe impl Mmio32 for Regs {
    #[inline]
    fn addr(&self) -> usize {
        Self::base_addr() + *self as usize
    }
}

#[repr(align(16))]
pub struct Payload<const N: usize>([u32; N]);

#[allow(dead_code)]
#[repr(u32)]
#[derive(Debug, Clone, Copy)]
pub enum RawTag {
    End = 0,

    GetSerial = 0x00010004,

    SetPower = 0x00028001,

    GetClockState = 0x00030001,
    SetClockState = 0x00038001,
    GetClockRate = 0x00030002,
    SetClockRate = 0x00038002,
    GetEdid = 0x00030020,

    GetFb = 0x00040001,
    GetPhysicalWH = 0x00040003,
    SetPhysicalWH = 0x00048003,
    SetVirtualWH = 0x00048004,
    SetDepth = 0x00048005,
    SetPixelOrder = 0x00048006,
    GetPitch = 0x00040008,
    GetVirtualOffset = 0x00040009,
    SetVirtualOffset = 0x00048009,
    GetOverscan = 0x0004000A,
    SetOverscan = 0x0004800A,
}

impl RawTag {
    #[inline]
    pub const fn as_u32(&self) -> u32 {
        *self as u32
    }
}

#[allow(dead_code)]
#[repr(u32)]
#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Copy)]
pub enum ClockId {
    EMMC = 0x01,
    UART = 0x02,
    ARM = 0x03,
    CORE = 0x04,
    V3D = 0x05,
    H264 = 0x06,
    ISP = 0x07,
    SDRAM = 0x08,
    PIXEL = 0x09,
    PWM = 0x0a,
    HEVC = 0x0b,
    EMMC2 = 0x0c,
    M2MC = 0x0d,
    PIXEL_BVB = 0x00e,
}

#[allow(dead_code)]
pub enum Tag {
    SetClockRate(ClockId, u32, u32),
    GetPhysicalWH,
    SetPhysicalWH(u32, u32),
    SetVirtualWH(u32, u32),
    GetVirtualOffset,
    SetVirtualOffset(u32, u32),
    SetDepth(u32),
    SetPixelOrder(PixelOrder),
    GetFb(u32),
    GetPitch,
    GetEdid(u32),
    GetOverscan,
    SetOverscan(u32, u32, u32, u32),
}

impl Tag {
    #[inline]
    const fn info(&self) -> (RawTag, u32) {
        match *self {
            Tag::SetClockRate(_, _, _) => (RawTag::SetClockRate, 12),
            Tag::GetPhysicalWH => (RawTag::GetPhysicalWH, 8),
            Tag::SetPhysicalWH(_, _) => (RawTag::SetPhysicalWH, 8),
            Tag::SetVirtualWH(_, _) => (RawTag::SetVirtualWH, 8),
            Tag::GetVirtualOffset => (RawTag::GetVirtualOffset, 8),
            Tag::SetVirtualOffset(_, _) => (RawTag::SetVirtualOffset, 8),
            Tag::SetDepth(_) => (RawTag::SetDepth, 4),
            Tag::SetPixelOrder(_) => (RawTag::SetPixelOrder, 4),
            Tag::GetFb(_) => (RawTag::GetFb, 8),
            Tag::GetPitch => (RawTag::GetPitch, 4),
            Tag::GetOverscan => (RawTag::GetOverscan, 16),
            Tag::SetOverscan(_, _, _, _) => (RawTag::SetOverscan, 16),
            Tag::GetEdid(_) => (RawTag::GetEdid, 136),
        }
    }

    pub fn append_to<const N: usize>(
        &self,
        slice: &mut MboxContext<Request, N>,
    ) -> Result<usize, ()> {
        let (tag, len1) = self.info();
        let new_len = slice.index + (len1 as usize + 3) / 4 + 3;

        slice._push(tag.as_u32())?;
        slice._push(len1)?;
        let result = slice._push(0)?;

        let index = match *self {
            Tag::SetClockRate(x, y, z) => slice._push_slice(&[x as u32, y, z]),
            Tag::SetPhysicalWH(x, y) => slice._push_slice(&[x, y]),
            Tag::SetVirtualWH(x, y) => slice._push_slice(&[x, y]),
            Tag::SetVirtualOffset(x, y) => slice._push_slice(&[x, y]),
            Tag::SetDepth(x) => slice._push(x),
            Tag::SetPixelOrder(x) => slice._push(x as u32),
            Tag::GetFb(x) => slice._push_slice(&[x, 0]),
            Tag::GetPhysicalWH => slice._push_dummy(2),
            Tag::GetVirtualOffset => slice._push_dummy(2),
            Tag::GetPitch => slice._push_dummy(1),
            Tag::GetOverscan => slice._push_dummy(4),
            Tag::SetOverscan(a, b, c, d) => slice._push_slice(&[a, b, c, d]),
            Tag::GetEdid(x) => slice._push(x).and_then(|_| slice._push_dummy(33)),
        }?;

        assert_eq!(new_len, index);

        Ok(result)
    }
}

#[allow(dead_code)]
#[repr(u32)]
#[derive(Debug, Copy, Clone)]
pub enum PixelOrder {
    BGR = 0,
    RGB = 1,
}
