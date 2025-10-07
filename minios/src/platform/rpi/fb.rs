use super::mbox::{Mbox, PixelOrder, Tag};
use crate::*;
use core::mem::transmute;
use edid::Edid;

#[allow(dead_code)]
pub struct Fb;

#[allow(dead_code)]
impl Fb {
    pub fn init(width: u32, height: u32) -> Result<(*mut u32, u32, u32, usize), ()> {
        let mut mbox = Mbox::PROP.new::<35>();
        mbox.append(Tag::SetPhysicalWH(width, height))?;
        mbox.append(Tag::SetVirtualWH(width, height))?;
        mbox.append(Tag::SetVirtualOffset(0, 0))?;
        mbox.append(Tag::SetDepth(32))?;
        mbox.append(Tag::SetPixelOrder(PixelOrder::BGR))?;
        let index_fb = mbox.append(Tag::GetFb(4096))?;
        let index_pitch = mbox.append(Tag::GetPitch)?;

        match mbox.call() {
            Ok(mbox) => {
                let ptr = (mbox.response(index_fb) & 0x3fff_ffff) as usize as *mut u32;
                let stride = mbox.response(index_pitch) as usize / 4;
                Ok((ptr, width, height, stride))
            }
            Err(_) => Err(()),
        }
    }

    #[track_caller]
    pub fn get_default_size() -> (u32, u32) {
        let mut mbox = Mbox::PROP.new::<8>();
        let index_pwh = mbox.append(Tag::GetPhysicalWH).unwrap();

        match mbox.call() {
            Ok(mbox) => {
                let w = mbox.response(index_pwh);
                let h = mbox.response(index_pwh + 1);
                (w, h)
            }
            Err(_) => {
                panic!("Failed to get default display size");
            }
        }
    }

    pub fn get_edid_size(result: Option<&mut [u8; 128]>) -> Result<(u32, u32), ()> {
        let mut mbox = Mbox::PROP.new::<40>();
        let index_edid = mbox.append(Tag::GetEdid(0))?;

        match mbox.call() {
            Ok(mbox) => {
                let edid: &[u32; 32] = mbox.response_slice(index_edid + 2);
                let edid: &[u8; 128] = unsafe { transmute(edid) };
                let edid = Edid::new(edid).ok_or(())?;

                let (x, y) = edid.active_pixels();

                if let Some(result) = result {
                    result.copy_from_slice(edid.as_slice());
                }

                Ok((x as u32, y as u32))
            }
            Err(_) => Err(()),
        }
    }

    pub fn set_overscan(top: u32, bottom: u32, left: u32, right: u32) -> Result<(), ()> {
        let mut mbox = Mbox::PROP.new::<10>();
        mbox.append(Tag::SetOverscan(top, bottom, left, right))?;
        match mbox.call() {
            Ok(_) => Ok(()),
            Err(_) => Err(()),
        }
    }

    pub fn get_fb() -> Result<(PhysicalAddress, usize), ()> {
        let mut mbox = Mbox::PROP.new::<8>();
        let index_fb = mbox.append(Tag::GetFb(0))?;

        match mbox.call() {
            Ok(mbox) => {
                let ptr =
                    PhysicalAddress::from_usize((mbox.response(index_fb) & 0x3FFF_FFFF) as usize);
                let size = mbox.response(index_fb + 1) as usize;
                Ok((ptr, size))
            }
            Err(_) => Err(()),
        }
    }
}
