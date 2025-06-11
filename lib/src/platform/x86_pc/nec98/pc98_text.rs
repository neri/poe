//! PC98 Text Mode Driver

use crate::{
    io::tty::{SimpleTextOutput, SimpleTextOutputMode},
    *,
};
use core::{arch::asm, cell::UnsafeCell};

const COLOR_TABLE: [u8; 8] = [0, 1, 4, 5, 2, 3, 6, 7];

pub struct Pc98Text {
    mode: SimpleTextOutputMode,
    line_height_m1: u8,
    native_attribute: u8,
}

static mut PC98_TEXT: UnsafeCell<Pc98Text> = UnsafeCell::new(Pc98Text {
    mode: SimpleTextOutputMode {
        columns: 80,
        rows: 25,
        cursor_column: 0,
        cursor_row: 0,
        attribute: 0,
        cursor_visible: 0,
    },
    native_attribute: 0,
    line_height_m1: 15,
});

impl Pc98Text {
    pub(super) unsafe fn init() {
        unsafe {
            let stdout = (&mut *(&raw mut PC98_TEXT)).get_mut();
            stdout.reset();
            System::set_stdout(stdout);

            // UNSAFE: aliasing mutable static
            let stderr = (&mut *(&raw mut PC98_TEXT)).get_mut();
            System::set_stderr(stderr);
        }
    }

    #[inline]
    fn get_vram(&self) -> *mut u8 {
        0xa0000 as *mut u8
    }

    unsafe fn gdc_command(command: u8, params: &[u8]) {
        unsafe {
            let mut guard = Hal::cpu().interrupt_guard();
            loop {
                let status: u8;
                asm!(
                    "in al, 0x60",
                    out("al") status,
                );
                if status & 0x04 != 0 {
                    break;
                }
                guard = Hal::cpu().interrupt_guard();
                asm!("out 0x5f, al");
            }

            asm!("out 0x62, al", in("al") command);

            for data in params {
                asm!("out 0x60, al", in("al") *data);
            }

            drop(guard);
        }
    }

    fn set_hw_cursor_visible(&mut self, visible: bool) {
        unsafe {
            let lh = self.line_height_m1;
            if visible {
                Self::gdc_command(0x4b, &[lh | 0x80, 0, lh << 3 | 3]);
            } else {
                Self::gdc_command(0x4b, &[lh, 0, 0]);
            }
        }
    }

    fn set_hw_cursor_position(&mut self, col: u8, row: u8) {
        unsafe {
            let pos = self.pos(col, row);
            Self::gdc_command(0x49, &[pos as u8, (pos >> 8) as u8]);
        }
    }

    #[inline]
    const fn pos(&self, col: u8, row: u8) -> usize {
        row as usize * self.mode.columns as usize + col as usize
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
            let vram = self.get_vram() as *mut u16;
            unsafe {
                let count = (self.mode.columns as usize * (self.mode.rows as usize - 1)) / 2;
                let src = vram.offset(self.mode.columns as isize);
                let mut dst = vram;
                asm!(
                    "xchg esi, {0}",
                    "rep movsd",
                    "xchg esi, {0}",
                    inout(reg) src => _,
                    inout("ecx") count => _,
                    inout("edi") dst,
                );
                let count = self.mode.columns as usize / 2;
                asm!(
                    "rep stosd",
                    inout("ecx") count => _,
                    inout("edi") dst,
                    in("eax") 0x20_0020,
                );
                let _ = dst;
            }
            row -= 1;
        }

        Some((col, row))
    }
}

impl core::fmt::Write for Pc98Text {
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
                    let ch = if ch == '\\' {
                        0xfc
                    } else if ch >= ' ' && ch < '\x7f' {
                        ch as u8
                    } else {
                        b'?'
                    };

                    if let Some((new_col, new_row)) = self.adjust_coords(col, row, true) {
                        col = new_col;
                        row = new_row;
                        pos = self.pos(col, row);
                    }

                    unsafe {
                        let offset = pos as isize * 2;
                        let vram = self.get_vram().offset(offset);
                        vram.offset(0x2000).write_volatile(self.native_attribute);
                        vram.write_volatile(ch);
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

impl SimpleTextOutput for Pc98Text {
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

        if self.mode.attribute < 0x10 {
            self.native_attribute = (COLOR_TABLE[self.mode.attribute as usize & 0x07]) << 5 | 0x01;
        } else {
            self.native_attribute =
                (COLOR_TABLE[(self.mode.attribute as usize >> 4) & 0x7]) << 5 | 0x05;
        }
    }

    fn clear_screen(&mut self) {
        let old_cursor_visible = self.enable_cursor(false);

        let vram_base = self.get_vram();
        unsafe {
            asm!(
                "rep stosd",
                inout("ecx") 80 * 25 / 2 => _,
                inout("edi") vram_base => _,
                in("eax") 0x20_0020,
            );
            asm!(
                "rep stosd",
                inout("ecx") 80 * 25 / 2 => _,
                inout("edi") vram_base.add(0x2000) => _,
                in("eax") (self.native_attribute as u32) * 0x01_0001,
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
