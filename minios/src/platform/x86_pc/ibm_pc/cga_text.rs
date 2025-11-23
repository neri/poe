//! CGA Text Mode Driver

use crate::{
    System,
    arch::{
        cpu::Cpu,
        vm86::{VM86, X86StackContext},
    },
    io::tty::{SimpleTextOutput, SimpleTextOutputMode},
    platform::x86_pc::ibm_pc::bios::INT10,
};
use core::cell::UnsafeCell;
use x86::isolated_io::*;

pub struct CgaText {
    mode: SimpleTextOutputMode,
    max_scan_line: u8,
    attr_mask: u8,
    is_vga: bool,
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
    max_scan_line: 0,
    attr_mask: 0x7f,
    is_vga: false,
});

impl CgaText {
    pub(super) unsafe fn init() {
        unsafe {
            let stdout = (&mut *(&raw mut CGA_TEXT)).get_mut();

            let cols = (0x44a as *const u8).read_volatile();
            stdout.mode.columns = cols;
            let rows = (0x484 as *const u8).read_volatile();
            if rows > 0 {
                stdout.mode.rows = rows + 1;
            }
            stdout.max_scan_line = CRTC::MaxScanLine.read() & 0x1f;

            stdout.reset();
            System::set_stdout(stdout);

            // UNSAFE: aliasing mutable static
            let stderr = (&mut *(&raw mut CGA_TEXT)).get_mut();
            System::set_stderr(stderr);
        }
    }

    pub(super) unsafe fn init_late() {
        unsafe {
            let stdout = (&mut *(&raw mut CGA_TEXT)).get_mut();

            let mut regs = X86StackContext::default();
            regs.eax.set_d(0x1a00);
            VM86::call_bios(INT10, &mut regs);
            if regs.eax.b() == 0x1a {
                // vga or later
                stdout.is_vga = true;

                // turn off blinking
                AttributeController::Mode.write(0x00);
                stdout.attr_mask = 0xff;
            }
        }
    }

    #[inline]
    fn get_vram(&self) -> *mut u8 {
        0xb8000 as *mut u8
    }

    fn set_hw_cursor_visible(&mut self, visible: bool) {
        unsafe {
            if visible {
                CRTC::CursorStartScanLine.write(self.max_scan_line - 1);
                CRTC::CursorEndScanLine.write(self.max_scan_line);
            } else {
                CRTC::CursorStartScanLine.write(0x20);
            }
        }
    }

    fn set_hw_cursor_position(&mut self, col: u8, row: u8) {
        unsafe {
            let pos = self.pos(col, row);
            CRTC::CursorLocationLow.write(pos as u8);
            CRTC::CursorLocationHigh.write((pos >> 8) as u8);
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
        if self.is_vga {
            // turn off blinking
            unsafe {
                AttributeController::Mode.write(0x00);
            }
        }
        self.set_attribute(0);
        self.mode.set_cursor_visible(true);
        self.clear_screen();
    }

    fn set_attribute(&mut self, attribute: u8) {
        self.mode.attribute = if attribute == 0 {
            System::DEFAULT_STDOUT_ATTRIBUTE & self.attr_mask
        } else {
            attribute & self.attr_mask
        };
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

    fn current_mode(&mut self) -> SimpleTextOutputMode {
        self.mode.clone()
    }
}

#[allow(dead_code)]
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CRTC {
    HorizontalTotal = 0x00,
    HorizontalDisplayEnd = 0x01,
    StartHorizontalBlank = 0x02,
    EndHorizontalBlank = 0x03,
    StartHorizontalRetrace = 0x04,
    EndHorizontalRetrace = 0x05,
    VerticalTotal = 0x06,
    Overflow = 0x07,
    PresetRowScan = 0x08,
    MaxScanLine = 0x09,
    CursorStartScanLine = 0x0a,
    CursorEndScanLine = 0x0b,
    StartAddressHigh = 0x0c,
    StartAddressLow = 0x0d,
    CursorLocationHigh = 0x0e,
    CursorLocationLow = 0x0f,
}

#[allow(dead_code)]
impl CRTC {
    const VGA_CRTC_IDX_DAT: IoPortWW = IoPortWW(0x3d4);
    const VGA_CRTC_INDEX: IoPortWB = IoPortWB(0x3d4);
    const VGA_CRTC_DATA: IoPortRWB = IoPortRWB(0x3d5);

    #[inline]
    unsafe fn write(&self, data: u8) {
        unsafe {
            Self::VGA_CRTC_IDX_DAT.write(u16::from_le_bytes([*self as u8, data]));
        }
    }

    #[inline]
    unsafe fn read(&self) -> u8 {
        unsafe {
            Self::VGA_CRTC_INDEX.write(*self as u8);
            Self::VGA_CRTC_DATA.read()
        }
    }
}

#[allow(dead_code)]
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttributeController {
    Mode = 0x10,
}

#[allow(dead_code)]
impl AttributeController {
    const VGA_AC_IDX_DAT: IoPortRWB = IoPortRWB(0x3c0);
    const VGA_AC_READ: IoPortRB = IoPortRB(0x3c1);

    #[inline]
    unsafe fn write(&self, data: u8) {
        unsafe {
            let _ = IoPortRB(0x3da).read();
            let old = Self::VGA_AC_IDX_DAT.read();
            Self::VGA_AC_IDX_DAT.write(*self as u8);
            Self::VGA_AC_IDX_DAT.write(data);
            Self::VGA_AC_IDX_DAT.write(old);
            let _ = IoPortRB(0x3da).read();
        }
    }

    #[inline]
    unsafe fn read(&self) -> u8 {
        unsafe {
            let _ = IoPortRB(0x3da).read();
            Self::VGA_AC_IDX_DAT.write(*self as u8);
            Self::VGA_AC_READ.read()
        }
    }
}
