// NEC PC-98 Series Computer Dependent

use super::pic::Irq;
use crate::io::hid::*;

static mut PC98: Pc98 = Pc98::new();

pub struct Pc98 {
    key_modifier: Modifier,
    mouse_state: MouseState,
}

impl Pc98 {
    const PORT_MOUSE_CTRL: usize = 0x7FDD;
    const PORT_MOUSE_READ: usize = 0x7FD9;

    const fn new() -> Self {
        Self {
            key_modifier: Modifier::empty(),
            mouse_state: MouseState::empty(),
        }
    }

    #[inline]
    fn shared<'a>() -> &'a mut Self {
        unsafe { &mut PC98 }
    }

    pub unsafe fn init() {
        Irq(1).register(Self::irq1).unwrap();
        Irq(13).register(Self::irq13).unwrap();

        // MOUSE = 120Hz
        asm!("out dx, al", in("edx") 0xBFDB, in("al") 0x00u8);
        // INT6, ENABLE INT
        asm!("out dx, al", in("edx") 0x98D7, in("al") 0x0Du8);
        // MOUSE RESET
        asm!("out dx, al", in("edx") 0x7FDF, in("al") 0x93u8);
    }

    /// IRQ1 Standard Keyboard
    fn irq1(_irq: Irq) {
        let shared = Self::shared();
        unsafe {
            let data: u8;
            asm!("
                mov al, 0x16
                out 0x43, al
                out 0x5F, al
                in al, 0x41
                ", out ("al") data);
            shared.process_key_data(data);
        }
    }

    /// IRQ13 Bus Mouse
    fn irq13(_irq: Irq) {
        let shared = Self::shared();
        unsafe {
            let mut c0: u8;
            asm!("in al, dx", in("edx") Self::PORT_MOUSE_CTRL, out("al") c0);

            c0 = (c0 & 0x0F) | 0x90;
            asm!("out dx, al", in("edx") Self::PORT_MOUSE_CTRL, in("al") c0);
            let mut m0: u8;
            asm!("in al, dx", in("edx") Self::PORT_MOUSE_READ, out("al") m0);

            c0 = c0 | 0x20;
            asm!("out dx, al", in("edx") Self::PORT_MOUSE_CTRL, in("al") c0);
            let mut m1: u8;
            asm!("in al, dx", in("edx") Self::PORT_MOUSE_READ, out("al") m1);

            c0 = (c0 & 0x9F) | 0x40;
            asm!("out dx, al", in("edx") Self::PORT_MOUSE_CTRL, in("al") c0);
            let mut m2: u8;
            asm!("in al, dx", in("edx") Self::PORT_MOUSE_READ, out("al") m2);

            c0 = c0 | 0x20;
            asm!("out dx, al", in("edx") Self::PORT_MOUSE_CTRL, in("al") c0);
            let mut m3: u8;
            asm!("in al, dx", in("edx") Self::PORT_MOUSE_READ, out("al") m3);

            c0 &= 0x0F;
            asm!("out dx, al", in("edx") Self::PORT_MOUSE_CTRL, in("al") c0);

            let buttons: MouseButton = if (m0 & 0x80) == 0 {
                MouseButton::LEFT
            } else {
                MouseButton::empty()
            } | if (m0 & 0x20) == 0 {
                MouseButton::RIGHT
            } else {
                MouseButton::empty()
            } | if (m0 & 0x40) == 0 {
                MouseButton::MIDDLE
            } else {
                MouseButton::empty()
            };
            let x = ((m1 << 4) | (m0 & 0x0F)) as i8;
            let y = ((m3 << 4) | (m2 & 0x0F)) as i8;
            let report = MouseReport { buttons, x, y };
            shared.mouse_state.process_mouse_report(report);
        }
    }

    fn process_key_data(&mut self, data: u8) {
        let is_break = (data & 0x80) != 0;
        let scancode = (data & 0x7F) as usize;
        let flags = if is_break {
            KeyEventFlags::BREAK
        } else {
            KeyEventFlags::empty()
        };
        let usage = Usage(unsafe { *SCAN_TO_HID.get_unchecked(scancode) });
        if usage >= Usage::MOD_MIN && usage < Usage::MOD_MAX {
            let bit_position =
                unsafe { Modifier::from_bits_unchecked(1 << (usage.0 - Usage::MOD_MIN.0)) };
            self.key_modifier.set(bit_position, !is_break);
            KeyEvent::new(Usage::NONE, self.key_modifier, flags).post();
        } else {
            KeyEvent::new(usage, self.key_modifier, flags).post();
        }
    }
}

// Keyboard scan code to HID usage table
static SCAN_TO_HID: [u8; 128] = [
    0x29, 0x1E, 0x1F, 0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x2D, 0x2E, 0x89, 0x2A, 0x2B,
    0x14, 0x1A, 0x08, 0x15, 0x17, 0x1C, 0x18, 0x0C, 0x12, 0x13, 0x2F, 0x30, 0x28, 0x04, 0x16, 0x07,
    0x09, 0x0A, 0x0B, 0x0D, 0x0E, 0x0F, 0x33, 0x34, 0x31, 0x1D, 0x1B, 0x06, 0x19, 0x05, 0x11, 0x10,
    0x36, 0x37, 0x38, 0x87, 0x2C, 0x8A, 0x4B, 0x4E, 0x49, 0x4C, 0x52, 0x50, 0x4F, 0x51, 0x4A, 0x4D,
    0x56, 0x54, 0x5F, 0x60, 0x61, 0x55, 0x5C, 0x5D, 0x5E, 0x57, 0x59, 0x5A, 0x5B, 0x67, 0x62, 0x85,
    0x63, 0x8B, 0x44, 0x45, 0x68, 0x69, 0x6A, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x29, 0x00,
    0x48, 0x46, 0x3A, 0x3B, 0x3C, 0x3D, 0x3E, 0x3F, 0x40, 0x41, 0x42, 0x43, 0x00, 0x00, 0x00, 0x00,
    0xE1, 0x39, 0x88, 0xE2, 0xE0, 0xE5, 0x00, 0xE3, 0xE7, 0x65, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];
