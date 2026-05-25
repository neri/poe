//! UEFI Graphics Output Protocol (GOP) driver

use uefi::Identify;
use uefi::prelude::*;
use uefi::proto::console::gop;

use super::*;
use crate::io::graphics::*;
use crate::platform::uefi::console::UefiConsole;

/// UEFI Graphics Output Protocol (GOP) driver
pub struct UefiGop {
    gop: Handle,
    modes: Vec<ModeInfo>,
    bios_modes: Vec<gop::Mode>,
    current_mode: CurrentMode,
}

impl UefiGop {
    #[inline]
    const fn new(gop: Handle, modes: Vec<ModeInfo>, bios_modes: Vec<gop::Mode>) -> Self {
        Self {
            gop,
            modes,
            bios_modes,
            current_mode: CurrentMode::empty(),
        }
    }

    pub(super) unsafe fn init() {
        unsafe {
            let Ok(handle_buffer) = uefi::boot::locate_handle_buffer(
                uefi::boot::SearchType::ByProtocol(&gop::GraphicsOutput::GUID),
            ) else {
                // GOP not found
                return;
            };
            let handle_gop = handle_buffer[0];
            let gop = get_protocol::<gop::GraphicsOutput>(handle_gop).unwrap();

            let mode_info = gop.current_mode_info();
            let preferred_graphics_mode = PreferredGraphicsMode {
                width: mode_info.resolution().0 as u16,
                height: mode_info.resolution().1 as u16,
                pixel_format: PixelFormat::BGRX8888,
            };

            let bios_modes = gop
                .modes()
                .filter(|mode| matches!(mode.info().pixel_format(), gop::PixelFormat::Bgr))
                .collect::<Vec<_>>();
            if bios_modes.is_empty() {
                // No compatible modes found
                return;
            }
            let modes = bios_modes
                .iter()
                .map(|mode| mode_info_from_gop_mode(mode.info()))
                .collect::<Vec<_>>();

            let driver = Box::new(Self::new(handle_gop, modes, bios_modes));
            System::conctl().set_graphics(driver as Box<dyn GraphicsOutputDevice>);
            System::conctl().set_preferred_graphics_mode(preferred_graphics_mode);
        }
    }
}

#[inline]
fn mode_info_from_gop_mode(gop_mode: &gop::ModeInfo) -> ModeInfo {
    ModeInfo {
        width: gop_mode.resolution().0 as u16,
        height: gop_mode.resolution().1 as u16,
        bytes_per_scanline: gop_mode.stride() as u16 * 4,
        pixel_format: PixelFormat::BGRX8888,
    }
}

impl GraphicsOutputDevice for UefiGop {
    fn modes(&self) -> &[ModeInfo] {
        &self.modes
    }

    fn current_mode(&self) -> &CurrentMode {
        &self.current_mode
    }

    fn set_mode(&mut self, mode: ModeIndex) -> Result<(), ()> {
        unsafe {
            let mode_info = *self.modes.get(mode.0 as usize).ok_or(())?;
            let bios_mode = *self.bios_modes.get(mode.0 as usize).ok_or(())?;

            let mut gop = get_protocol::<gop::GraphicsOutput>(self.gop).unwrap();
            gop.set_mode(&bios_mode).map_err(|_| ())?;
            let mut fb = gop.frame_buffer();

            self.current_mode = CurrentMode {
                current: mode,
                info: mode_info,
                fb: PhysicalAddress::from_ptr(fb.as_mut_ptr() as *const _),
                fb_size: fb.size() as usize,
            };
            Ok(())
        }
    }

    fn detach(&mut self) {
        unsafe {
            let p = self.current_mode.fb.as_usize() as *mut u8;
            p.write_bytes(0, self.current_mode.fb_size);
        }

        UefiConsole::handover();
    }
}
