use core::mem::transmute;

use edid::Edid;

use super::mbox::{Mbox, PixelOrder, Tag};
use crate::io::graphics::color::IndexedColor;
use crate::io::graphics::*;
use crate::*;

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

        let default_mode = ModeInfo {
            width: width as u16,
            height: height as u16,
            bytes_per_scanline: (width * 4) as u16,
            pixel_format: PixelFormat::BGRX8888,
        };
        driver.modes.push(default_mode);
        System::conctl().set_preferred_graphics_mode(default_mode.into());
        for template in &[(320, 200), (320, 240), (640, 480), (800, 600), (1024, 768)] {
            driver.modes.push(ModeInfo {
                width: template.0 as u16,
                height: template.1 as u16,
                bytes_per_scanline: template.0 as u16,
                pixel_format: PixelFormat::Indexed8,
            });
        }
        for template in &[
            (640, 480),
            (800, 600),
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
        System::conctl().set_preferred_graphics_mode(default_mode.into());
    }

    /// Sets the resolution and pixel format.
    /// Returns a pointer to the framebuffer, width, height, and stride in bytes.
    pub fn set_resolution(
        width: u32,
        height: u32,
        pixel_format: PixelFormat,
    ) -> Result<(*mut u32, u32, u32, usize), ()> {
        match pixel_format {
            PixelFormat::BGRX8888 | PixelFormat::RGBX8888 => {
                let mut mbox = Mbox::PROP.fixed::<35>();
                mbox.append(Tag::SetPhysicalWH(width, height))?;
                mbox.append(Tag::SetVirtualWH(width, height))?;
                mbox.append(Tag::SetVirtualOffset(0, 0))?;

                mbox.append(Tag::SetDepth(32))?;
                match pixel_format {
                    PixelFormat::BGRX8888 => {
                        mbox.append(Tag::SetPixelOrder(PixelOrder::BGR))?;
                    }
                    PixelFormat::RGBX8888 => {
                        mbox.append(Tag::SetPixelOrder(PixelOrder::RGB))?;
                    }
                    _ => unreachable!(),
                }

                let index_fb = mbox.append(Tag::GetFb(4096))?;
                let index_pitch = mbox.append(Tag::GetPitch)?;

                match mbox.call() {
                    Ok(mbox) => {
                        let stride = mbox.response(index_pitch) as usize;
                        let ptr = (mbox.response(index_fb) & 0x3fff_ffff) as usize as *mut u32;
                        Ok((ptr, width, height, stride))
                    }
                    Err(_) => Err(()),
                }
            }
            PixelFormat::Indexed8 => {
                let (pw, ph, vw, vh, adjust_offset) = match (width, height) {
                    // (320, 200) => (320, 240, 320, 240, 20),
                    _ => (width, height, width, height, 0),
                };

                let mut mbox = Mbox::PROP.alloc(280);
                mbox.append(Tag::SetPhysicalWH(pw, ph))?;
                mbox.append(Tag::SetVirtualWH(vw, vh))?;
                mbox.append(Tag::SetVirtualOffset(0, 0))?;

                mbox.append(Tag::SetDepth(8))?;
                mbox.append(Tag::SetPixelOrder(PixelOrder::RGB))?;
                // Convert the palette because the RPi firmware expects another format.
                let mut palette = Box::new(IndexedColor::COLOR_PALETTE.clone());
                palette.iter_mut().for_each(|color| {
                    *color = ((*color >> 16) & 0xff) | (*color & 0xff00) | ((*color & 0xff) << 16);
                });
                mbox.append(Tag::SetPalette(&palette))?;

                let index_fb = mbox.append(Tag::GetFb(4096))?;
                let index_pitch = mbox.append(Tag::GetPitch)?;

                match mbox.call() {
                    Ok(mbox) => {
                        let stride = mbox.response(index_pitch) as usize;
                        let ptr = ((mbox.response(index_fb) as usize & 0x3fff_ffff)
                            + (adjust_offset * stride))
                            as *mut u32;
                        Ok((ptr, width, height, stride))
                    }
                    Err(_) => Err(()),
                }
            }
        }
    }

    pub fn get_default_size() -> (u32, u32) {
        let mut mbox = Mbox::PROP.fixed::<8>();
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

    /// Gets the EDID data and returns preferred resolution.
    /// If `result` is `Some`, the EDID data will be copied into the provided buffer.
    pub fn get_edid_size(result: Option<&mut [u8; 128]>) -> Result<(u32, u32), ()> {
        let mut mbox = Mbox::PROP.fixed::<40>();
        let index_edid = mbox.append(Tag::GetEdid(0)).unwrap();

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
        let mut mbox = Mbox::PROP.fixed::<10>();
        mbox.append(Tag::SetOverscan(top, bottom, left, right))?;
        match mbox.call() {
            Ok(_) => Ok(()),
            Err(_) => Err(()),
        }
    }

    pub fn get_fb() -> Result<(PhysicalAddress, usize), ()> {
        let mut mbox = Mbox::PROP.fixed::<8>();
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
        let mut info = *self.modes.get(mode.0 as usize).ok_or(())?;
        if let Ok((ptr, _w, h, stride)) =
            Fb::set_resolution(info.width as u32, info.height as u32, info.pixel_format)
        {
            info.bytes_per_scanline = stride as u16;
            self.current_mode = CurrentMode {
                current: mode,
                info,
                fb: PhysicalAddress::from_usize(ptr as usize),
                fb_size: (stride * h as usize),
            };
            Ok(())
        } else {
            Err(())
        }
    }

    fn detach(&mut self) {
        unsafe {
            let p = self.current_mode.fb.as_usize() as *mut u8;
            let size = self.current_mode.fb_size;
            p.write_bytes(0, size);
        }
    }
}
