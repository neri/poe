//! UEFI Graphics Output Protocol (GOP) driver

use crate::io::graphics::*;
use crate::platform::uefi::console::UefiConsole;
use crate::*;
use uefi::{
    Identify,
    boot::{OpenProtocolParams, ScopedProtocol},
    prelude::*,
    proto::console::gop::{self, Mode},
};

pub struct UefiGop {
    gop: Handle,
    modes: Vec<ModeInfo>,
    bios_modes: Vec<Mode>,
    current_mode: CurrentMode,
}

impl UefiGop {
    #[inline]
    const fn new(gop: Handle, modes: Vec<ModeInfo>, bios_modes: Vec<Mode>) -> Self {
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
            let gop = open_gop(handle_gop).unwrap();

            let bios_modes = gop
                .modes()
                .filter(|mode| matches!(mode.info().pixel_format(), gop::PixelFormat::Bgr))
                .collect::<Vec<_>>();
            let modes = bios_modes
                .iter()
                .map(|mode| mode_info_from_gop_mode(mode.info()))
                .collect::<Vec<_>>();

            let driver = Box::new(Self::new(handle_gop, modes, bios_modes));
            System::conctl().set_graphics(driver as Box<dyn GraphicsOutputDevice>);
        }
    }
}

unsafe fn open_gop(handle: Handle) -> Option<ScopedProtocol<gop::GraphicsOutput>> {
    unsafe {
        let hinsatnce = uefi::boot::image_handle();
        uefi::boot::open_protocol::<gop::GraphicsOutput>(
            OpenProtocolParams {
                handle: handle,
                agent: hinsatnce,
                controller: None,
            },
            uefi::boot::OpenProtocolAttributes::GetProtocol,
        )
        .ok()
    }
}

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

            let mut gop = open_gop(self.gop).unwrap();
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
