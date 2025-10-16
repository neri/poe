//! VESA BIOS Extensions (VBE) support

use super::bios::INT10;
use crate::arch::{
    lomem::LoMemoryManager,
    vm86::{VM86, X86StackContext},
};
use crate::io::graphics::*;
use crate::*;
use alloc::collections::BinaryHeap;
use x86::isolated_io::IoPortWB;

pub struct VesaBios {
    modes: Vec<ModeInfo>,
    bios_modes: Vec<u16>,
    current_mode: CurrentMode,
}

impl VesaBios {
    #[inline]
    const fn new() -> Self {
        Self {
            modes: Vec::new(),
            bios_modes: Vec::new(),
            current_mode: CurrentMode::empty(),
        }
    }

    pub(super) unsafe fn init() {
        unsafe {
            let buffer = LoMemoryManager::alloc_page();

            let mut regs = X86StackContext::default();
            regs.eax.set_d(0x4f00);
            regs.set_vmes(buffer.sel());
            regs.edi.set_d(0);
            VM86::call_bios(INT10, &mut regs);
            if regs.eax.w() != 0x004f {
                // VESA BIOS not supported
                return;
            }

            let mut modes = BinaryHeap::new();
            modes.push(ModeInfoInner {
                bios_mode: 0x13,
                info: ModeInfo {
                    width: 320,
                    height: 200,
                    bytes_per_scanline: 320,
                    pixel_format: PixelFormat::Indexed8,
                },
            });

            for mode in 0x100..0x200 {
                regs.eax.set_d(0x4f01);
                regs.ecx.set_d(mode);
                regs.set_vmes(buffer.sel());
                regs.edi.set_d(0);
                VM86::call_bios(INT10, &mut regs);
                if regs.eax.w() != 0x004f {
                    continue;
                }
                let mode_info = &*buffer.base().as_ptr::<VbeModeInfo>();

                if mode_info.attributes & 0x99 != 0x99 {
                    // not supported, not graphics, not linear framebuffer
                    continue;
                }
                let pixel_format = match mode_info.pixel_format() {
                    Some(v) => v,
                    None => continue,
                };

                let mode = ModeInfoInner {
                    bios_mode: mode as u16,
                    info: ModeInfo {
                        width: mode_info.width,
                        height: mode_info.height,
                        bytes_per_scanline: mode_info.bytes_per_scanline,
                        pixel_format,
                    },
                };
                modes.push(mode);
            }

            let modes = modes.into_sorted_vec();
            let mut driver = Box::new(Self::new());
            driver.modes = modes.iter().map(|v| v.info).collect::<Vec<_>>();
            driver.bios_modes = modes.iter().map(|v| v.bios_mode).collect::<Vec<_>>();

            System::console_controller().set_graphics(driver as Box<dyn GraphicsOutput>);
        }
    }
}

impl GraphicsOutput for VesaBios {
    fn modes(&self) -> &[ModeInfo] {
        &self.modes
    }

    fn current_mode(&self) -> &CurrentMode {
        &self.current_mode
    }

    fn set_mode(&mut self, mode: ModeIndex) -> Result<(), ()> {
        unsafe {
            let bios_mode = *self.bios_modes.get(mode.0 as usize).ok_or(())?;
            let info = *self.modes.get(mode.0 as usize).ok_or(())?;

            let buffer = LoMemoryManager::alloc_page();
            let mut regs = X86StackContext::default();
            regs.eax.set_d(0x4f01);
            regs.ecx.set_d(bios_mode as u32);
            regs.set_vmes(buffer.sel());
            regs.edi.set_d(0);
            VM86::call_bios(INT10, &mut regs);
            if regs.eax.w() != 0x004f {
                return Err(());
            }
            let vbe_mode_info = &*buffer.base().as_ptr::<VbeModeInfo>();
            let fb = vbe_mode_info.phys_base_ptr as usize;
            let fb_size = info.bytes_per_scanline as usize * info.height as usize;

            regs.eax.set_d(0x4f02);
            regs.ebx.set_d(0x4000 | bios_mode as u32);
            VM86::call_bios(INT10, &mut regs);
            if regs.eax.w() != 0x004f {
                return Err(());
            }

            if info.pixel_format.is_indexed_color() {
                for (i, &color) in COLOR_PALETTE.iter().enumerate() {
                    IoPortWB(0x3c8).write(i as u8);
                    IoPortWB(0x3c9).write((color >> 18) as u8 & 0x3f);
                    IoPortWB(0x3c9).write((color >> 10) as u8 & 0x3f);
                    IoPortWB(0x3c9).write((color >> 2) as u8 & 0x3f);
                }
            }

            let current_mode = CurrentMode {
                current: mode,
                info,
                fb: PhysicalAddress::from_usize(fb),
                fb_size,
            };
            self.current_mode = current_mode;
            Ok(())
        }
    }

    fn detach(&mut self) {
        unsafe {
            let mut regs = X86StackContext::default();
            regs.eax.set_d(0x0003);
            VM86::call_bios(INT10, &mut regs);
        }
    }
}

#[repr(C)]
#[allow(unused)]
pub struct VbeModeInfo {
    attributes: u16,
    win_a_attributes: u8,
    win_b_attributes: u8,
    win_granularity: u16,
    win_size: u16,
    win_a_segment: u16,
    win_b_segment: u16,
    win_func_ptr: u32,
    bytes_per_scanline: u16,
    width: u16,
    height: u16,
    x_char_size: u8,
    y_char_size: u8,
    number_of_planes: u8,
    bits_per_pixel: u8,
    number_of_banks: u8,
    memory_model: u8,
    bank_size: u8,
    number_of_image_pages: u8,
    _reserved1: u8,
    red_mask_size: u8,
    red_field_position: u8,
    green_mask_size: u8,
    green_field_position: u8,
    blue_mask_size: u8,
    blue_field_position: u8,
    reserved_mask_size: u8,
    reserved_field_position: u8,
    direct_color_mode_info: u8,
    phys_base_ptr: u32,
    off_screen_mem_offset: u32,
    off_screen_mem_size: u16,
    _reserved2: [u8; 206],
}

impl VbeModeInfo {
    #[inline]
    pub fn pixel_format(&self) -> Option<PixelFormat> {
        match (self.bits_per_pixel, self.memory_model) {
            (8, 0x04) => return Some(PixelFormat::Indexed8),
            // (15, 0x06) => {}
            // (16, 0x06) => {}
            // (24, 0x06) => {}
            (32, 0x06) => {}
            // other modes are not supported
            _ => return None,
        }
        let magic = u64::from_le_bytes([
            self.red_mask_size,
            self.red_field_position,
            self.green_mask_size,
            self.green_field_position,
            self.blue_mask_size,
            self.blue_field_position,
            self.reserved_mask_size,
            self.reserved_field_position,
        ]);
        match magic {
            0x1808_0008_0808_1008 => Some(PixelFormat::BGRX8888),
            0x1808_1008_0808_0008 => Some(PixelFormat::RGBX8888),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ModeInfoInner {
    bios_mode: u16,
    info: ModeInfo,
}

impl PartialOrd for ModeInfoInner {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ModeInfoInner {
    #[inline]
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        match self.info.width.cmp(&other.info.width) {
            core::cmp::Ordering::Equal => {}
            ord => return ord,
        }
        match self.info.height.cmp(&other.info.height) {
            core::cmp::Ordering::Equal => {}
            ord => return ord,
        }
        match (self.info.pixel_format as u32).cmp(&(other.info.pixel_format as u32)) {
            core::cmp::Ordering::Equal => {}
            ord => return ord,
        }
        self.bios_mode.cmp(&other.bios_mode)
    }
}
