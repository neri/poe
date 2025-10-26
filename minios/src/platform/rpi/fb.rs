use super::mbox::{Mbox, PixelOrder, Tag};
use crate::io::graphics::*;
use crate::*;
use core::mem::transmute;
use edid::Edid;

pub struct Fb {
    modes: Vec<ModeInfo>,
    current_mode: CurrentMode,
}

#[allow(unused)]
impl Fb {
    pub(super) unsafe fn init() {
        let mut driver = Box::new(Self {
            modes: Vec::new(),
            current_mode: CurrentMode::empty(),
        });

        let _ = Self::set_overscan(0, 0, 0, 0);
        let width;
        let height;
        let mut edid = [0; 128];
        if let Ok((x, y)) = Self::get_edid_size(Some(&mut edid)) {
            width = x;
            height = y;

            println!("EDID: ");
            for (i, &v) in edid.iter().enumerate() {
                print!(" {:02x}", v);
                if (i & 15) == 15 {
                    println!("");
                }
            }
        } else {
            (width, height) = Self::get_default_size();
        }

        driver.modes.push(ModeInfo {
            width: width as u16,
            height: height as u16,
            bytes_per_scanline: (width * 4) as u16,
            pixel_format: PixelFormat::BGRX8888,
        });
        for template in &[
            (800, 600),
            (640, 480),
            (320, 200),
            (1024, 768),
            (1280, 720),
            (1920, 1080),
        ] {
            if template.0 != width && template.1 != height {
                driver.modes.push(ModeInfo {
                    width: template.0 as u16,
                    height: template.1 as u16,
                    bytes_per_scanline: (template.0 * 4) as u16,
                    pixel_format: PixelFormat::BGRX8888,
                });
            }
        }

        System::conctl().set_graphics(driver as Box<dyn GraphicsOutputDevice>);
    }

    pub fn set_resolution(width: u32, height: u32) -> Result<(*mut u32, u32, u32, usize), ()> {
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

impl GraphicsOutputDevice for Fb {
    fn modes(&self) -> &[ModeInfo] {
        &self.modes
    }

    fn current_mode(&self) -> &CurrentMode {
        &self.current_mode
    }

    fn set_mode(&mut self, mode: ModeIndex) -> Result<(), ()> {
        let info = *self.modes.get(mode.0 as usize).ok_or(())?;
        if let Ok((ptr, _w, h, stride)) = Fb::set_resolution(info.width as u32, info.height as u32)
        {
            self.current_mode = CurrentMode {
                current: mode,
                info,
                fb: PhysicalAddress::from_usize(ptr as usize),
                fb_size: (stride * h as usize * 4),
            };
            Ok(())
        } else {
            Err(())
        }
    }

    fn detach(&mut self) {
        // todo: nothing to do
    }
}
