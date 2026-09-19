//! Keyboard matrix of Chromebooks (8 rows x 13 columns), scanned by the ChromeOS EC
//!
//! This module has no hardware dependency.

use libhid::layouts::KeyStroke;
use libhid::{Modifier, Usage};

pub const COLUMNS: usize = 13;
pub const ROWS: usize = 8;

/// HID usages of the standard Chromebook keyboard, indexed by `[column][row]`
///
/// Generated from `CROS_STD_TOP_ROW_KEYMAP` and `CROS_STD_MAIN_KEYMAP` of Linux
/// `include/dt-bindings/input/cros-ec-keyboard.h`. It includes the JIS keys (RO, YEN, HENKAN,
/// MUHENKAN) and the ISO key (102ND). The top row is F1-F10, and SLEEP (the lock key) is ignored.
static KEYMAP: [[u8; ROWS]; COLUMNS] = [
    //   row 0          1          2          3          4          5          6          7
    [0x00, 0x00, 0xe0, 0xe3, 0xe4, 0x00, 0x00, 0x00], // col  0: - - LEFTCTRL LEFTMETA RIGHTCTRL - - -
    [0xe3, 0x29, 0x2b, 0x35, 0x04, 0x1d, 0x1e, 0x14], // col  1: LEFTMETA ESC TAB GRAVE A Z 1 Q
    [0x3a, 0x3d, 0x3c, 0x3b, 0x07, 0x06, 0x20, 0x08], // col  2: F1 F4 F3 F2 D C 3 E
    [0x05, 0x0a, 0x17, 0x22, 0x09, 0x19, 0x21, 0x15], // col  3: B G T 5 F V 4 R
    [0x43, 0x40, 0x3f, 0x3e, 0x16, 0x1b, 0x1f, 0x1a], // col  4: F10 F7 F6 F5 S X 2 W
    [0x87, 0x00, 0x30, 0x00, 0x0e, 0x36, 0x25, 0x0c], // col  5: RO - RIGHTBRACE - K COMMA 8 I
    [0x11, 0x0b, 0x1c, 0x23, 0x0d, 0x10, 0x24, 0x18], // col  6: N H Y 6 J M 7 U
    [0x00, 0x00, 0x64, 0x00, 0x00, 0xe1, 0x00, 0xe5], // col  7: - - 102ND - - LEFTSHIFT - RIGHTSHIFT
    [0x2e, 0x34, 0x2f, 0x2d, 0x33, 0x38, 0x27, 0x13], // col  8: EQUAL APOSTROPHE LEFTBRACE MINUS SEMICOLON SLASH 0 P
    [0x00, 0x42, 0x41, 0x00, 0x0f, 0x37, 0x26, 0x12], // col  9: - F9 F8 SLEEP L DOT 9 O
    [0xe6, 0x00, 0x89, 0x00, 0x31, 0x00, 0xe2, 0x00], // col 10: RIGHTALT - YEN - BACKSLASH - LEFTALT -
    [0x00, 0x2a, 0x00, 0x31, 0x28, 0x2c, 0x51, 0x52], // col 11: - BACKSPACE - BACKSLASH ENTER SPACE DOWN UP
    [0x00, 0x8a, 0x00, 0x8b, 0x00, 0x00, 0x4f, 0x50], // col 12: - HENKAN - MUHENKAN - - RIGHT LEFT
];

/// State of the keyboard matrix
pub struct KeyMatrix {
    /// One byte per column, one bit per row
    last: [u8; COLUMNS],
}

impl KeyMatrix {
    #[inline]
    pub const fn new() -> Self {
        Self { last: [0; COLUMNS] }
    }

    /// Returns the HID usage of the key at the position.
    #[inline]
    pub fn usage(column: usize, row: usize) -> Usage {
        Usage(KEYMAP[column][row])
    }

    /// Updates the state without reporting any key (e.g. to discard stale events).
    #[inline]
    pub fn reset(&mut self, state: &[u8; COLUMNS]) {
        self.last = *state;
    }

    /// Updates the state and calls `f` for each newly pressed key other than the modifiers,
    /// with the modifiers held at the moment. Keys held down are not repeated.
    ///
    /// As depthcharge does, if a pressed key shares its row with another pressed key and
    /// its column with yet another one, the state may contain ghost keys,
    /// so nothing is reported and `false` is returned.
    pub fn update(&mut self, state: &[u8; COLUMNS], mut f: impl FnMut(KeyStroke)) -> bool {
        let last = core::mem::replace(&mut self.last, *state);

        let mut modifier = 0u8;
        let mut pressed = [(0u8, 0u8); COLUMNS * ROWS];
        let mut count = 0;
        for column in 0..COLUMNS {
            for row in 0..ROWS {
                if (state[column] & (1 << row)) == 0 {
                    continue;
                }
                let usage = Self::usage(column, row);
                if usage >= Usage::MOD_MIN && usage <= Usage::MOD_MAX {
                    modifier |= 1 << (usage.0 - Usage::MOD_MIN.0);
                }
                pressed[count] = (column as u8, row as u8);
                count += 1;
            }
        }
        let pressed = &pressed[..count];

        let is_ghost = pressed.iter().enumerate().any(|(i, &(column, row))| {
            let others = pressed.iter().enumerate().filter(|&(j, _)| j != i);
            others.clone().any(|(_, &(_, r))| r == row)
                && others.clone().any(|(_, &(c, _))| c == column)
        });
        if is_ghost {
            return false;
        }

        let modifier = Modifier::from_bits_retain(modifier);
        for &(column, row) in pressed {
            let (column, row) = (column as usize, row as usize);
            let usage = Self::usage(column, row);
            let is_new = (last[column] & (1 << row)) == 0;
            if is_new
                && usage != Usage::NONE
                && !(usage >= Usage::MOD_MIN && usage <= Usage::MOD_MAX)
            {
                f(KeyStroke::new(usage, modifier));
            }
        }
        true
    }
}
