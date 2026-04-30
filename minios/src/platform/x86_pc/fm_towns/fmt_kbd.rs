//! FM TOWNS Keyboard Driver

use crate::io::hid_mgr::{HidManager, KeyStroke};
use crate::platform::x86_pc::pic::Irq;
use crate::*;
use core::cell::UnsafeCell;
use libhid::*;
use x86::isolated_io::{IoPortRB, IoPortWB};

static mut FMT_KBD: UnsafeCell<FmtKbd> = UnsafeCell::new(FmtKbd::new());

pub struct FmtKbd {
    leading_data: KbdLeadingData,
    key_modifier: Modifier,
    key_buffer: heapless::Vec<KeyStroke, 16>,
}

impl FmtKbd {
    const fn new() -> Self {
        Self {
            leading_data: KbdLeadingData::empty(),
            key_modifier: Modifier::empty(),
            key_buffer: heapless::Vec::new(),
        }
    }

    #[inline]
    unsafe fn shared_mut<'a>() -> &'a mut Self {
        unsafe { (&mut *(&raw mut FMT_KBD)).get_mut() }
    }

    pub unsafe fn init() {
        unsafe {
            HidManager::set_japanese_layout();

            IoPortWB(0x0604).write(0x01);
            IoPortWB(0x0602).write(0xa1);
            IoPortWB(0x0604).write(0x01);

            Irq(1).register(Self::irq1).unwrap();

            System::set_stdin(Self::shared_mut());
        }
    }

    /// IRQ1 Standard Keyboard
    fn irq1(_irq: Irq) {
        unsafe {
            let shared = Self::shared_mut();
            let _ = IoPortRB(0x0602).read();
            let data = IoPortRB(0x0600).read();
            let leading = KbdLeadingData::from_bits_retain(data);
            if leading.is_leading() {
                shared.leading_data = leading;
            } else {
                shared.process_key_data(data);
            }
        }
    }

    fn process_key_data(&mut self, data: u8) {
        let leading = self.leading_data;
        if leading.contains(KbdLeadingData::EXTEND) {
            return;
        }
        let is_break = leading.is_break();
        if !is_break {
            self.key_modifier.set(
                Modifier::LEFT_CTRL,
                leading.contains(KbdLeadingData::HAS_CTRL),
            );
            self.key_modifier.set(
                Modifier::LEFT_SHIFT,
                leading.contains(KbdLeadingData::HAS_SHIFT),
            );
            let usage = Usage(SCAN_TO_HID[0x7F & data as usize]);
            if usage >= Usage::MOD_MIN && usage < Usage::MOD_MAX {
                let bit_position = Modifier::from_bits_retain(1 << (usage.0 - Usage::MOD_MIN.0));
                self.key_modifier.set(bit_position, !is_break);
            } else {
                let key_stroke = KeyStroke::new(usage, self.key_modifier);
                let _ = self.key_buffer.push(key_stroke);
            }
        }
    }
}

impl SimpleTextInput for FmtKbd {
    fn reset(&mut self) {
        self.leading_data = KbdLeadingData::empty();
        self.key_modifier = Modifier::empty();
        self.key_buffer.clear();
    }

    fn is_ready(&mut self) -> bool {
        !self.key_buffer.is_empty()
    }

    fn read_key_stroke(&mut self) -> Option<NonZeroInputKey> {
        self.is_ready()
            .then(|| self.key_buffer.remove(0))
            .and_then(|key_stroke| InputKey::from_key_stroke(key_stroke).into())
    }
}

#[derive(Debug, Clone, Copy)]
struct KbdLeadingData(u8);

impl KbdLeadingData {
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
        self.contains(KbdLeadingData::IS_LEADING)
    }

    #[inline]
    pub const fn is_break(&self) -> bool {
        self.contains(KbdLeadingData::IS_BREAK)
    }
}

/// Keyboard scan code to HID usage table
#[rustfmt::skip]
static SCAN_TO_HID: [u8; 128] = [
    /*         -0    -1    -2    -3    -4    -5    -6    -7    -8    -9    -A    -B    -C    -D    -E    -F */
    /* 0- */    0, 0x29, 0x1e, 0x1f, 0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x2d, 0x2e, 0x89, 0x2a,
    /* 1- */ 0x2b, 0x14, 0x1a, 0x08, 0x15, 0x17, 0x1c, 0x18, 0x0c, 0x12, 0x13, 0x2f, 0x30, 0x28, 0x04, 0x16,
    /* 2- */ 0x07, 0x09, 0x0a, 0x0b, 0x0d, 0x0e, 0x0f, 0x33, 0x34, 0x31, 0x1d, 0x1b, 0x06, 0x19, 0x05, 0x11,
    /* 3- */ 0x10, 0x36, 0x37, 0x38, 0x87, 0x2c, 0x55, 0x54, 0x57, 0x56, 0x5f, 0x60, 0x61,    0, 0x5c, 0x5d,
    /* 4- */ 0x5e,    0, 0x59, 0x5a, 0x5b, 0x58, 0x62, 0x63, 0x4c,    0,    0, 0x4c,    0, 0x52, 0x4a, 0x50,
    /* 5- */ 0x51, 0x4f, 0xe0, 0xe1,    0, 0x39,    0, 0x8b, 0x8a,    0,    0, 0x45,    0, 0x3a, 0x3b, 0x3c,
    /* 6- */ 0x3d, 0x3e, 0x3f, 0x40, 0x41, 0x42, 0x43,    0,    0, 0x44,    0,    0,    0,    0,    0,    0,
    /* 7- */    0, 0x88,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0, 0x48, 0x46,    0,    0,
];
