use core::mem::transmute;

use super::mbox::{Mbox, PixelOrder, Tag};
use crate::*;

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
                let edid = mbox.response_slice::<32>(index_edid + 2);
                let edid: &[u8; 128] = unsafe { transmute(edid) };

                if edid[0..8] != [0x00, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x00] {
                    return Err(());
                }
                if edid.iter().fold(0u8, |acc, &x| acc.wrapping_add(x)) != 0 {
                    return Err(());
                }

                let x = edid[0x38] as u32 | ((edid[0x3a] as u32 & 0xf0) << 4);
                let y = edid[0x3b] as u32 | ((edid[0x3d] as u32 & 0xf0) << 4);

                if let Some(result) = result {
                    result.copy_from_slice(edid);
                }

                Ok((x, y))
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
