//! FM TOWNS Keyboard Driver

use crate::{platform::x86_pc::pic::Irq, *};
use core::cell::UnsafeCell;
use libhid::*;
use x86::isolated_io::IoPort;

static mut FMT_KBD: UnsafeCell<FmtKbd> = UnsafeCell::new(FmtKbd::new());

pub struct FmtKbd {
    lead_data: KbdLeadData,
    key_modifier: Modifier,
    last_key_data: Option<NonZeroInputKey>,
}

impl FmtKbd {
    const fn new() -> Self {
        Self {
            lead_data: KbdLeadData::empty(),
            key_modifier: Modifier::empty(),
            last_key_data: None,
        }
    }

    #[inline]
    unsafe fn shared<'a>() -> &'a mut Self {
        unsafe { (&mut *(&raw mut FMT_KBD)).get_mut() }
    }

    pub unsafe fn init() {
        unsafe {
            IoPort(0x0604).out8(0x01);
            IoPort(0x0602).out8(0xa1);
            IoPort(0x0604).out8(0x01);

            Irq(1).register(Self::irq1).unwrap();

            System::set_stdin(Self::shared());
        }
    }

    /// IRQ1 Standard Keyboard
    fn irq1(_irq: Irq) {
        unsafe {
            let shared = Self::shared();
            let _ = IoPort(0x0602).in8();
            let data = IoPort(0x0600).in8();
            let leading = KbdLeadData::from_bits_retain(data);
            if leading.is_leading() {
                shared.lead_data = leading;
            } else {
                shared.process_key_data(data);
            }
        }
    }

    fn process_key_data(&mut self, data: u8) {
        let leading = self.lead_data;
        if leading.contains(KbdLeadData::EXTEND) {
            return;
        }
        let is_break = leading.is_break();
        if !is_break {
            self.key_modifier
                .set(Modifier::LEFT_CTRL, leading.contains(KbdLeadData::HAS_CTRL));
            self.key_modifier.set(
                Modifier::LEFT_SHIFT,
                leading.contains(KbdLeadData::HAS_SHIFT),
            );
            let usage = Usage(SCAN_TO_HID[0x7F & data as usize]);
            if usage >= Usage::MOD_MIN && usage < Usage::MOD_MAX {
                let bit_position = Modifier::from_bits_retain(1 << (usage.0 - Usage::MOD_MIN.0));
                self.key_modifier.set(bit_position, !is_break);
                // KeyEvent::new(Usage::NONE, self.key_modifier, flags).post();
            } else {
                let ascii = SCAN_TO_ASCII[0x7F & data as usize];
                let ascii = match ascii {
                    0x21..=0x3f => {
                        if self.key_modifier.has_shift() {
                            ascii ^ 0x10
                        } else {
                            ascii
                        }
                    }
                    0x40..=0x7e => {
                        if self.key_modifier.has_ctrl() {
                            ascii & 0x1f
                        } else if self.key_modifier.has_shift() {
                            ascii ^ 0x20
                        } else {
                            ascii
                        }
                    }
                    _ => ascii,
                };
                self.last_key_data = InputKey {
                    unicode_char: ascii as u16,
                    usage: usage.0 as u16,
                }
                .into();
                // KeyEvent::new(usage, self.key_modifier, flags).post();
            }
        }
    }
}

impl SimpleTextInput for FmtKbd {
    fn reset(&mut self) {
        self.lead_data = KbdLeadData::empty();
        self.key_modifier = Modifier::empty();
        self.last_key_data = None;
    }

    fn read_key_stroke(&mut self) -> Option<NonZeroInputKey> {
        self.last_key_data.take()
    }
}

#[derive(Debug, Clone, Copy)]
struct KbdLeadData(u8);

impl KbdLeadData {
    pub const IS_LEADING: Self = Self(0b1000_0000);
    pub const EXTEND: Self = Self(0b0110_0000);
    pub const IS_BREAK: Self = Self(0b0001_0000);
    pub const HAS_CTRL: Self = Self(0b0000_1000);
    pub const HAS_SHIFT: Self = Self(0b0000_0100);

    #[inline]
    pub const fn empty() -> Self {
        Self(0)
    }

    #[inline]
    pub fn from_bits_retain(data: u8) -> Self {
        Self(data)
    }

    #[inline]
    pub const fn contains(&self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }

    #[inline]
    pub const fn is_leading(&self) -> bool {
        self.contains(KbdLeadData::IS_LEADING)
    }

    #[inline]
    pub const fn is_break(&self) -> bool {
        self.contains(KbdLeadData::IS_BREAK)
    }
}

// Keyboard scan code to HID usage table
static SCAN_TO_HID: [u8; 128] = [
    0x00, 0x29, 0x1E, 0x1F, 0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x2D, 0x2E, 0x89, 0x2A,
    0x2B, 0x14, 0x1A, 0x08, 0x15, 0x17, 0x1C, 0x18, 0x0C, 0x12, 0x13, 0x2F, 0x30, 0x28, 0x04, 0x16,
    0x07, 0x09, 0x0A, 0x0B, 0x0D, 0x0E, 0x0F, 0x33, 0x34, 0x31, 0x1D, 0x1B, 0x06, 0x19, 0x05, 0x11,
    0x10, 0x36, 0x37, 0x38, 0x87, 0x2C, 0x55, 0x54, 0x57, 0x56, 0x5F, 0x60, 0x61, 0x00, 0x5C, 0x5D,
    0x5E, 0x00, 0x59, 0x5A, 0x5B, 0x58, 0x62, 0x63, 0x4C, 0x00, 0x00, 0x4C, 0x00, 0x52, 0x4A, 0x50,
    0x51, 0x4F, 0xE0, 0xE1, 0x00, 0x39, 0x00, 0x8B, 0x8A, 0x00, 0x00, 0x45, 0x00, 0x3A, 0x3B, 0x3C,
    0x3D, 0x3E, 0x3F, 0x40, 0x41, 0x42, 0x43, 0x00, 0x00, 0x44, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x88, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x48, 0x46, 0x00, 0x00,
];

static SCAN_TO_ASCII: [u8; 128] = *b"\x00\x1b1234567890-^\\\x08\x09qwertyuiop@[\x0dasdfghjkl;:]zxcvbnm,./_                                                                           ";
