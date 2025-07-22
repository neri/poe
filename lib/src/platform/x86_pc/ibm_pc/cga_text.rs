//! CGA Text Mode Driver

use crate::{
    System,
    arch::cpu::Cpu,
    io::tty::{SimpleTextOutput, SimpleTextOutputMode},
};
use core::cell::UnsafeCell;
use x86::isolated_io::IoPortWW;

const VGA_CRTC_IDX_DAT: IoPortWW = IoPortWW(0x3d4);

pub struct CgaText {
    mode: SimpleTextOutputMode,
}

static mut CGA_TEXT: UnsafeCell<CgaText> = UnsafeCell::new(CgaText {
    mode: SimpleTextOutputMode {
        columns: 80,
        rows: 25,
        cursor_column: 0,
        cursor_row: 0,
        attribute: 0,
        cursor_visible: 0,
    },
});

impl CgaText {
    pub(super) unsafe fn init() {
        unsafe {
            let stdout = (&mut *(&raw mut CGA_TEXT)).get_mut();
            stdout.reset();
            System::set_stdout(stdout);

            // UNSAFE: aliasing mutable static
            let stderr = (&mut *(&raw mut CGA_TEXT)).get_mut();
            System::set_stderr(stderr);
        }
    }

    #[inline]
    fn get_vram(&self) -> *mut u8 {
        0xb8000 as *mut u8
    }

    #[inline]
    unsafe fn crtc_out(index: u8, data: u8) {
        unsafe {
            VGA_CRTC_IDX_DAT.write(u16::from_le_bytes([index, data]));
        }
    }

    fn set_hw_cursor_visible(&mut self, visible: bool) {
        unsafe {
            if visible {
                Self::crtc_out(0x0a, 0xce);
                Self::crtc_out(0x0b, 0xef);
            } else {
                Self::crtc_out(0x0a, 0x20);
            }
        }
    }

    fn set_hw_cursor_position(&mut self, col: u8, row: u8) {
        unsafe {
            let pos = self.pos(col, row);
            Self::crtc_out(0x0f, pos as u8);
            Self::crtc_out(0x0e, (pos >> 8) as u8);
        }
    }

    #[inline]
    const fn pos(&self, col: u8, row: u8) -> usize {
        row as usize * self.mode.columns as usize + col as usize
    }

    fn adjust_coords(&self, col: u8, row: u8, wrap_around: bool) -> Option<(u8, u8)> {
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
                let dst = self.get_vram() as *mut u32;
                let (dst, _) = Cpu::rep_movsd(
                    dst,
                    dst.byte_offset(2 * self.mode.columns as isize),
                    (self.mode.columns as usize * (self.mode.rows as usize - 1)) / 2,
                );
                Cpu::rep_stosd(
                    dst,
                    (0x20 | (self.mode.attribute as u32) << 8) * 0x10001,
                    self.mode.columns as usize / 2,
                );
            }
            row -= 1;
        }

        Some((col, row))
    }
}

impl core::fmt::Write for CgaText {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
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
                    let ch = if ch >= ' ' && ch < '\x7F' { ch } else { '?' };

                    if let Some((new_col, new_row)) = self.adjust_coords(col, row, true) {
                        col = new_col;
                        row = new_row;
                        pos = self.pos(col, row);
                    }

                    unsafe {
                        let offset = pos as isize * 2;
                        let vram = self.get_vram().offset(offset);
                        vram.write_volatile(ch as u8);
                        vram.offset(1).write_volatile(self.mode.attribute);
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
        if self.mode.is_cursor_visible() {
            self.set_hw_cursor_position(col, row);
        }

        Ok(())
    }
}

impl SimpleTextOutput for CgaText {
    fn reset(&mut self) {
        self.set_attribute(0);
        self.mode.set_cursor_visible(true);
        self.clear_screen();
    }

    fn set_attribute(&mut self, attribute: u8) {
        if attribute == 0 {
            self.mode.attribute = System::DEFAULT_STDOUT_ATTRIBUTE;
        } else {
            self.mode.attribute = attribute;
        }
    }

    fn clear_screen(&mut self) {
        let old_cursor_visible = self.mode.is_cursor_visible();

        unsafe {
            Cpu::rep_stosd(
                self.get_vram() as *mut u32,
                (0x20 | (self.mode.attribute as u32) << 8) * 0x10001,
                (self.mode.columns as usize * self.mode.rows as usize) / 2,
            );
        }

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
            self.set_hw_cursor_position(self.mode.cursor_column, self.mode.cursor_row);
            self.set_hw_cursor_visible(old_cursor_visible);
        }
    }

    fn enable_cursor(&mut self, visible: bool) -> bool {
        let old_value = self.mode.is_cursor_visible();
        if visible && !old_value {
            self.set_hw_cursor_position(self.mode.cursor_column, self.mode.cursor_row);
        }
        self.mode.set_cursor_visible(visible);
        self.set_hw_cursor_visible(visible);
        old_value
    }

    fn current_mode(&self) -> SimpleTextOutputMode {
        self.mode.clone()
    }
}
