// Fujitsu FM Towns Dependent

use super::pic::*;
use crate::io::hid::*;
use bitflags::*;

static mut TOWNS: FmTowns = FmTowns::new();

pub struct FmTowns {
    key_lead_data: KbdLeadData,
    key_modifier: Modifier,
    mouse_state: MouseState,
}

impl FmTowns {
    const fn new() -> Self {
        Self {
            key_lead_data: KbdLeadData::empty(),
            key_modifier: Modifier::empty(),
            mouse_state: MouseState::empty(),
        }
    }

    #[inline]
    fn shared<'a>() -> &'a mut Self {
        unsafe { &mut TOWNS }
    }

    pub unsafe fn init() {
        asm!("out dx, al", in("edx") 0x0604, in("al") 0x01u8);
        asm!("out dx, al", in("edx") 0x0602, in("al") 0xA1u8);
        asm!("out dx, al", in("edx") 0x0604, in("al") 0x01u8);

        Irq(1).register(Self::irq1).unwrap();

        Irq(11).register(Self::irq11).unwrap();
    }

    /// IRQ1 Standard Keyboard
    fn irq1(_irq: Irq) {
        let shared = Self::shared();
        unsafe {
            let data: u8;
            asm!("in al, dx", in("edx") 0x0602, lateout("al") _);
            asm!("in al, dx", in("edx") 0x0600, lateout("al") data);
            let leading = KbdLeadData::from_bits_unchecked(data);
            if leading.is_leading() {
                shared.key_lead_data = leading;
            } else {
                shared.process_keydata(data);
            }
        }
    }

    /// IRQ11 VSYNC (mouse polling)
    fn irq11(_irq: Irq) {
        let shared = Self::shared();
        shared.poll_mouse();
    }

    #[inline]
    fn process_keydata(&mut self, data: u8) {
        let leading = self.key_lead_data;
        if leading.contains(KbdLeadData::EXTEND) {
            return;
        }
        let is_break = leading.is_break();
        let flags = if is_break {
            KeyEventFlags::BREAK
        } else {
            KeyEventFlags::empty()
        };
        self.key_modifier
            .set(Modifier::LCTRL, leading.contains(KbdLeadData::HAS_CTRL));
        self.key_modifier
            .set(Modifier::LSHIFT, leading.contains(KbdLeadData::HAS_SHIFT));
        let usage = Usage(unsafe { *SCAN_TO_HID.get_unchecked(data as usize) });
        if usage >= Usage::MOD_MIN && usage < Usage::MOD_MAX {
            let bit_position =
                unsafe { Modifier::from_bits_unchecked(1 << (usage.0 - Usage::MOD_MIN.0)) };
            self.key_modifier.set(bit_position, !is_break);
            KeyEvent::new(Usage::NONE, self.key_modifier, flags).post();
        } else {
            KeyEvent::new(usage, self.key_modifier, flags).post();
        }
    }

    /// TODO: adjust polling timing
    #[inline]
    fn poll_mouse(&mut self) {
        unsafe {
            asm!("out dx, al", in("edx") 0x04D6, in("al") 0b0010_1100u8);
            let p0: u8;
            asm!("in al, dx", in("edx") 0x04D2, out("al") p0);

            asm!("out dx, al", in("edx") 0x04D6, in("al") 0b0000_0000u8);
            let p1: u8;
            asm!("in al, dx", in("edx") 0x04D2, out("al") p1);

            asm!("out dx, al", in("edx") 0x04D6, in("al") 0b0010_0000u8);
            let p2: u8;
            asm!("in al, dx", in("edx") 0x04D2, out("al") p2);

            asm!("out dx, al", in("edx") 0x04D6, in("al") 0b0000_0000u8);
            let p3: u8;
            asm!("in al, dx", in("edx") 0x04D2, out("al") p3);

            let buttons: MouseButton = if (p0 & 0x10) == 0 {
                MouseButton::LEFT
            } else {
                MouseButton::empty()
            } | if (p0 & 0x20) == 0 {
                MouseButton::RIGHT
            } else {
                MouseButton::empty()
            };
            let x = 0 - ((p0 << 4) | (p1 & 0x0F)) as i8;
            let y = 0 - ((p2 << 4) | (p3 & 0x0F)) as i8;
            let report = MouseReport { buttons, x, y };
            self.mouse_state.process_mouse_report(report);
        }
    }
}

bitflags! {
    struct KbdLeadData: u8 {
        const IS_LEADING    = 0b1000_0000;
        const EXTEND        = 0b0110_0000;
        const IS_BREAK      = 0b0001_0000;
        const HAS_CTRL      = 0b0000_1000;
        const HAS_SHIFT     = 0b0000_0100;
    }
}

impl KbdLeadData {
    fn is_leading(&self) -> bool {
        self.contains(KbdLeadData::IS_LEADING)
    }

    fn is_break(&self) -> bool {
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
