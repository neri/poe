//! FM TOWNS Text Mode Driver

use crate::{
    System,
    arch::cpu::Cpu,
    io::tty::{SimpleTextOutput, SimpleTextOutputMode},
    platform::x86_pc::fm_towns::crtc::Crtc,
};
use core::{cell::UnsafeCell, mem::transmute};
use x86::isolated_io::*;

const TVRAM_OFFSET_MASK: usize = 0x0003_ffff / 4;

const BITMAP_CONVERT_TABLE: [u32; 256] = [
    0x00000000, 0xf0000000, 0x0f000000, 0xff000000, 0x00f00000, 0xf0f00000, 0x0ff00000, 0xfff00000,
    0x000f0000, 0xf00f0000, 0x0f0f0000, 0xff0f0000, 0x00ff0000, 0xf0ff0000, 0x0fff0000, 0xffff0000,
    0x0000f000, 0xf000f000, 0x0f00f000, 0xff00f000, 0x00f0f000, 0xf0f0f000, 0x0ff0f000, 0xfff0f000,
    0x000ff000, 0xf00ff000, 0x0f0ff000, 0xff0ff000, 0x00fff000, 0xf0fff000, 0x0ffff000, 0xfffff000,
    0x00000f00, 0xf0000f00, 0x0f000f00, 0xff000f00, 0x00f00f00, 0xf0f00f00, 0x0ff00f00, 0xfff00f00,
    0x000f0f00, 0xf00f0f00, 0x0f0f0f00, 0xff0f0f00, 0x00ff0f00, 0xf0ff0f00, 0x0fff0f00, 0xffff0f00,
    0x0000ff00, 0xf000ff00, 0x0f00ff00, 0xff00ff00, 0x00f0ff00, 0xf0f0ff00, 0x0ff0ff00, 0xfff0ff00,
    0x000fff00, 0xf00fff00, 0x0f0fff00, 0xff0fff00, 0x00ffff00, 0xf0ffff00, 0x0fffff00, 0xffffff00,
    0x000000f0, 0xf00000f0, 0x0f0000f0, 0xff0000f0, 0x00f000f0, 0xf0f000f0, 0x0ff000f0, 0xfff000f0,
    0x000f00f0, 0xf00f00f0, 0x0f0f00f0, 0xff0f00f0, 0x00ff00f0, 0xf0ff00f0, 0x0fff00f0, 0xffff00f0,
    0x0000f0f0, 0xf000f0f0, 0x0f00f0f0, 0xff00f0f0, 0x00f0f0f0, 0xf0f0f0f0, 0x0ff0f0f0, 0xfff0f0f0,
    0x000ff0f0, 0xf00ff0f0, 0x0f0ff0f0, 0xff0ff0f0, 0x00fff0f0, 0xf0fff0f0, 0x0ffff0f0, 0xfffff0f0,
    0x00000ff0, 0xf0000ff0, 0x0f000ff0, 0xff000ff0, 0x00f00ff0, 0xf0f00ff0, 0x0ff00ff0, 0xfff00ff0,
    0x000f0ff0, 0xf00f0ff0, 0x0f0f0ff0, 0xff0f0ff0, 0x00ff0ff0, 0xf0ff0ff0, 0x0fff0ff0, 0xffff0ff0,
    0x0000fff0, 0xf000fff0, 0x0f00fff0, 0xff00fff0, 0x00f0fff0, 0xf0f0fff0, 0x0ff0fff0, 0xfff0fff0,
    0x000ffff0, 0xf00ffff0, 0x0f0ffff0, 0xff0ffff0, 0x00fffff0, 0xf0fffff0, 0x0ffffff0, 0xfffffff0,
    0x0000000f, 0xf000000f, 0x0f00000f, 0xff00000f, 0x00f0000f, 0xf0f0000f, 0x0ff0000f, 0xfff0000f,
    0x000f000f, 0xf00f000f, 0x0f0f000f, 0xff0f000f, 0x00ff000f, 0xf0ff000f, 0x0fff000f, 0xffff000f,
    0x0000f00f, 0xf000f00f, 0x0f00f00f, 0xff00f00f, 0x00f0f00f, 0xf0f0f00f, 0x0ff0f00f, 0xfff0f00f,
    0x000ff00f, 0xf00ff00f, 0x0f0ff00f, 0xff0ff00f, 0x00fff00f, 0xf0fff00f, 0x0ffff00f, 0xfffff00f,
    0x00000f0f, 0xf0000f0f, 0x0f000f0f, 0xff000f0f, 0x00f00f0f, 0xf0f00f0f, 0x0ff00f0f, 0xfff00f0f,
    0x000f0f0f, 0xf00f0f0f, 0x0f0f0f0f, 0xff0f0f0f, 0x00ff0f0f, 0xf0ff0f0f, 0x0fff0f0f, 0xffff0f0f,
    0x0000ff0f, 0xf000ff0f, 0x0f00ff0f, 0xff00ff0f, 0x00f0ff0f, 0xf0f0ff0f, 0x0ff0ff0f, 0xfff0ff0f,
    0x000fff0f, 0xf00fff0f, 0x0f0fff0f, 0xff0fff0f, 0x00ffff0f, 0xf0ffff0f, 0x0fffff0f, 0xffffff0f,
    0x000000ff, 0xf00000ff, 0x0f0000ff, 0xff0000ff, 0x00f000ff, 0xf0f000ff, 0x0ff000ff, 0xfff000ff,
    0x000f00ff, 0xf00f00ff, 0x0f0f00ff, 0xff0f00ff, 0x00ff00ff, 0xf0ff00ff, 0x0fff00ff, 0xffff00ff,
    0x0000f0ff, 0xf000f0ff, 0x0f00f0ff, 0xff00f0ff, 0x00f0f0ff, 0xf0f0f0ff, 0x0ff0f0ff, 0xfff0f0ff,
    0x000ff0ff, 0xf00ff0ff, 0x0f0ff0ff, 0xff0ff0ff, 0x00fff0ff, 0xf0fff0ff, 0x0ffff0ff, 0xfffff0ff,
    0x00000fff, 0xf0000fff, 0x0f000fff, 0xff000fff, 0x00f00fff, 0xf0f00fff, 0x0ff00fff, 0xfff00fff,
    0x000f0fff, 0xf00f0fff, 0x0f0f0fff, 0xff0f0fff, 0x00ff0fff, 0xf0ff0fff, 0x0fff0fff, 0xffff0fff,
    0x0000ffff, 0xf000ffff, 0x0f00ffff, 0xff00ffff, 0x00f0ffff, 0xf0f0ffff, 0x0ff0ffff, 0xfff0ffff,
    0x000fffff, 0xf00fffff, 0x0f0fffff, 0xff0fffff, 0x00ffffff, 0xf0ffffff, 0x0fffffff, 0xffffffff,
];

/// Video mode settings for FMR compatible mode
/// bg layer 0: mode 1 640x400 16colors planar
/// fg layer 1: mode 4 640x400 (1024x512 virtual) 16colors packed pixel
#[rustfmt::skip]
const TEXT_MODE_SETTINGS: [u16; 30] = [
    0x0040, 0x0320, /* ---   --- */ 0x035f, 0x0000, 0x0010, 0x0000,
    0x036f, 0x009c, 0x031c, 0x009c, 0x031c, 0x0040, 0x0360, 0x0040,
    0x0360, 0x0000, 0x009c, 0x0000, 0x0050, 0x0000, 0x009c, 0x0000,
    0x0080, 0x004a, 0x0001, 0x0000, 0x001f, 0x0003, 0x0000, 0x0150,
];

pub struct FmtText {
    mode: SimpleTextOutputMode,
    fg_color_u32: u32,
    bg_color_u32: u32,
    tvram_offset: usize,
    tvram_crtc_fa1: u16,
}

static mut FMT_TEXT: UnsafeCell<FmtText> = UnsafeCell::new(FmtText {
    mode: SimpleTextOutputMode {
        columns: 80,
        rows: 25,
        cursor_column: 0,
        cursor_row: 0,
        attribute: 0,
        cursor_visible: 0,
    },
    fg_color_u32: 0,
    bg_color_u32: 0,
    tvram_offset: 0,
    tvram_crtc_fa1: 0,
});

impl FmtText {
    pub(super) unsafe fn init() {
        unsafe {
            // Clear GVRAM
            let p = 0xc_ff81 as *mut u8;
            p.write_volatile(0x0f);
            Cpu::rep_stosd(0xc_0000 as *mut u32, 0, 80 * 400);

            let stdout = (&mut *(&raw mut FMT_TEXT)).get_mut();
            Self::hw_set_mode();
            stdout.reset();
            System::set_stdout(stdout);

            // UNSAFE: aliasing mutable static
            let stderr = (&mut *(&raw mut FMT_TEXT)).get_mut();
            System::set_stderr(stderr);
        }
    }

    pub unsafe fn hw_set_mode() {
        unsafe {
            Crtc::set_mode(&TEXT_MODE_SETTINGS, 0b0001_0101, 0b0000_1001, 0b0000_1111);
            IoPortWB(0xff99).write(0x01);
        }
    }

    #[inline]
    const fn get_vram(&self) -> *mut u32 {
        0x8004_0000 as *mut u32
    }

    #[inline]
    const fn get_base_font16(&self) -> *const u8 {
        // 0x000c_b000 as *const u8
        0xc213_d800 as *const u8
    }

    #[inline]
    const fn get_base_font8(&self) -> *const u8 {
        0x000c_a000 as *const u8
    }

    #[inline]
    fn set_hw_scroll(&mut self, offset: u16) {
        unsafe {
            Crtc::crtc_out(0x15, offset);
        }
    }

    fn flip_cursor(&mut self, col: u8, row: u8) {
        let pos = self.pos(col, row);
        unsafe {
            let mut vram = self
                .get_vram()
                .add((pos + self.tvram_offset) & TVRAM_OFFSET_MASK)
                .add(512 / 4 * 14);
            vram.write_volatile(!vram.read_volatile());
            vram = vram.add(512 / 4);
            vram.write_volatile(!vram.read_volatile());
        }
    }

    #[inline]
    const fn pos(&self, col: u8, row: u8) -> usize {
        row as usize * 512 / 4 * 16 as usize + col as usize
    }

    fn adjust_coords(&mut self, col: u8, row: u8, wrap_around: bool) -> Option<(u8, u8)> {
        if row < self.mode.rows && (!wrap_around || col < self.mode.columns) {
            return None;
        }

        let mut col = col;
        let mut row = row;

        if wrap_around {
            while col >= self.mode.columns {
                col -= self.mode.columns;
                row += 1;
            }
        }

        while row >= self.mode.rows {
            unsafe {
                let vram = self.get_vram().add(self.tvram_offset & TVRAM_OFFSET_MASK);
                Cpu::rep_stosd(vram, self.bg_color_u32, 512 * 16 / 4);
                self.tvram_offset = (self.tvram_offset + 512 * 16 / 4) & TVRAM_OFFSET_MASK;

                self.tvram_crtc_fa1 = self.tvram_crtc_fa1 + 1024 * 16 / 8;
                self.set_hw_scroll(self.tvram_crtc_fa1);
            }
            row -= 1;
        }

        Some((col, row))
    }

    fn octuple(value: u8) -> u32 {
        let q = value as u32 & 0x0F;
        q * 0x1111_1111
    }
}

impl core::fmt::Write for FmtText {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let old_cursor_visible = self.enable_cursor(false);

        let mut col = self.mode.cursor_column;
        let mut row = self.mode.cursor_row;
        let mut pos = self.pos(col, row);

        for ch in s.chars() {
            match ch {
                '\n' => {
                    col = 0;
                    row += 1;
                    if let Some((new_col, new_row)) = self.adjust_coords(col, row, false) {
                        col = new_col;
                        row = new_row;
                    }
                    pos = self.pos(col, row);
                }
                '\r' => {
                    col = 0;
                    pos = self.pos(col, row);
                }
                '\x08' => {
                    if col > 0 {
                        col -= 1;
                        pos -= 1;
                    }
                }
                _ => {
                    let ch = if ch >= ' ' && ch < '\x7F' {
                        ch as u32
                    } else {
                        b'?' as u32
                    };

                    if let Some((new_col, new_row)) = self.adjust_coords(col, row, true) {
                        col = new_col;
                        row = new_row;
                        pos = self.pos(col, row);
                    }

                    unsafe {
                        let mut vram = self
                            .get_vram()
                            .add((pos + self.tvram_offset) & TVRAM_OFFSET_MASK);

                        if true {
                            let font_data = self.get_base_font16().add(ch as usize * 16);
                            let font_data: &[u8; 16] = transmute(font_data);

                            for data in font_data {
                                let mask = BITMAP_CONVERT_TABLE[*data as usize];
                                let bg = self.bg_color_u32 & !mask;
                                let fg = self.fg_color_u32 & mask;
                                vram.write_volatile(bg | fg);
                                vram = vram.add(512 / 4);
                            }
                        } else {
                            let font_data = self.get_base_font8().add(ch as usize * 8);
                            let font_data: &[u8; 8] = transmute(font_data);

                            for data in font_data {
                                let mask = BITMAP_CONVERT_TABLE[*data as usize];
                                let bg = self.bg_color_u32 & !mask;
                                let fg = self.fg_color_u32 & mask;
                                vram.write_volatile(bg | fg);
                                vram = vram.add(512 / 4);
                                vram.write_volatile(bg | fg);
                                vram = vram.add(512 / 4);
                            }
                        }
                    }

                    col += 1;
                    pos += 1;
                }
            }
        }

        if let Some((new_col, new_row)) = self.adjust_coords(col, row, false) {
            col = new_col;
            row = new_row;
        }
        self.mode.cursor_column = col;
        self.mode.cursor_row = row;
        if old_cursor_visible {
            self.enable_cursor(old_cursor_visible);
        }

        Ok(())
    }
}

impl SimpleTextOutput for FmtText {
    fn reset(&mut self) {
        self.set_attribute(0);
        self.mode.set_cursor_visible(true);
        self.clear_screen();
    }

    fn set_attribute(&mut self, attribute: u8) {
        self.mode.attribute = if attribute == 0 {
            System::DEFAULT_STDOUT_ATTRIBUTE
        } else {
            attribute
        };
        self.fg_color_u32 = Self::octuple(self.mode.attribute & 0x0F);
        self.bg_color_u32 = Self::octuple((self.mode.attribute & 0xF0) >> 4);
    }

    fn clear_screen(&mut self) {
        let old_cursor_visible = self.mode.is_cursor_visible();

        unsafe {
            Cpu::rep_stosd(self.get_vram(), self.bg_color_u32, 512 * 1024 / 4);
        }

        self.tvram_offset = 0;
        self.tvram_crtc_fa1 = 0;
        self.set_hw_scroll(self.tvram_crtc_fa1);

        self.mode.cursor_column = 0;
        self.mode.cursor_row = 0;
        if old_cursor_visible {
            self.enable_cursor(old_cursor_visible);
        }
    }

    fn set_cursor_position(&mut self, col: u32, row: u32) {
        let old_cursor_visible = self.enable_cursor(false);
        self.mode.cursor_column = (self.mode.columns as u32).min(col) as u8;
        self.mode.cursor_row = (self.mode.rows as u32).min(row) as u8;
        if old_cursor_visible {
            self.flip_cursor(self.mode.cursor_column, self.mode.cursor_row);
        }
    }

    fn enable_cursor(&mut self, visible: bool) -> bool {
        let old_value = self.mode.is_cursor_visible();
        if visible != old_value {
            self.flip_cursor(self.mode.cursor_column, self.mode.cursor_row);
        }
        self.mode.set_cursor_visible(visible);
        old_value
    }

    fn current_mode(&mut self) -> SimpleTextOutputMode {
        self.mode.clone()
    }
}
