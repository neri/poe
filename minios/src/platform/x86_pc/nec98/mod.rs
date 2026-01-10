//! NEC PC-98 Series Computer
//!
//! # NOTE
//!
//! May not work or may need to be adjusted as it has not been fully verified on actual hardware.
//!

mod pc98_text;
mod pegc;

mod bios {
    use x86::prot::InterruptVector;

    /// Video and keyboard BIOS Services
    pub const INT18: InterruptVector = InterruptVector(0x18);

    #[allow(unused)]
    /// Disk BIOS Services
    pub const INT1B: InterruptVector = InterruptVector(0x1B);
}

use crate::arch::vm86::{VM86, X86StackContext};
use crate::io::hid_mgr::{HidManager, KeyStroke};
use crate::mem::{MemoryManager, MemoryType};
use crate::platform::x86_pc::pic::Irq;
use crate::*;
use libhid::{Modifier, Usage};
use x86::isolated_io::LoIoPortDummyB;

pub static PORT_5F: LoIoPortDummyB<0x5F> = LoIoPortDummyB::new();

pub(super) unsafe fn init(_info: &SsblInfo) {
    unsafe {
        pc98_text::Pc98Text::init();

        let _1mb = 0x0010_0000;
        let _15mb = 0x00f0_0000;
        let _16mb = 0x0100_0000;
        let mem_size = _1mb + ((0x0401 as *const u8).read_volatile() as u32) * 128 * 1024;
        let mem_size2 = (0x594 as *const u16).read_volatile() as u32 * _1mb;
        if mem_size <= _15mb {
            // 00f0_0000-00ff_ffff reserved area, like isa hole
            MemoryManager::register_memmap(_15mb as u64.._16mb as u64, MemoryType::Reserved)
                .unwrap();
        }
        if mem_size2 > 0 {
            MemoryManager::register_memmap(
                _16mb as u64..(_16mb + mem_size2) as u64,
                MemoryType::Available,
            )
            .unwrap();
        }

        arch::vm86::VM86::init();

        super::pic::Pic::init(
            0x00,
            0x02,
            0x08,
            0x0a,
            0b00010001,
            0x07,
            0b0001_1101,
            0b0000_1001,
            0b0111_1111_0111_1110,
            [
                0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15,
                0x16, 0x17,
            ],
        );

        super::pit::Pit::init(
            0x0071,
            0x3fdb,
            0x0077,
            2457,
            Irq(0),
            super::pit::Pit::advance_tick,
        );
        Hal::cpu().enable_interrupt();

        BiosTextInput::init();

        pegc::PegcBios::init();
    }
}

pub(super) unsafe fn exit() {
    // TODO:
}

static mut STDIN: BiosTextInput = BiosTextInput {};

struct BiosTextInput;

impl BiosTextInput {
    unsafe fn init() {
        unsafe {
            HidManager::set_japanese_layout();

            let kbd = &mut *(&raw mut STDIN);
            kbd.reset();
            System::set_stdin(kbd);
        }
    }
}

impl SimpleTextInput for BiosTextInput {
    fn reset(&mut self) {
        unsafe {
            let mut regs = X86StackContext::default();
            regs.eax.set_d(0x0300);
            VM86::call_bios(bios::INT18, &mut regs);
        }
    }

    fn is_ready(&mut self) -> bool {
        unsafe {
            let mut regs = X86StackContext::default();
            regs.eax.set_d(0x0100);
            VM86::call_bios(bios::INT18, &mut regs);
            regs.ebx.h() != 0
        }
    }

    fn read_key_stroke(&mut self) -> Option<NonZeroInputKey> {
        unsafe {
            let mut regs = X86StackContext::default();
            regs.eax.set_d(0x0100);
            VM86::call_bios(bios::INT18, &mut regs);
            if regs.ebx.h() == 0 {
                return None;
            }

            regs.eax.set_d(0);
            VM86::call_bios(bios::INT18, &mut regs);
            // let ch = match regs.eax.b() {
            //     ch @ 0x00..=0x7f => ch as char,
            //     _ => 0 as char,
            // };
            let usage = Usage(SCAN_TO_HID[0x7f & regs.eax.h() as usize]);

            regs.eax.set_d(0x0200);
            VM86::call_bios(bios::INT18, &mut regs);
            let mut modifier = Modifier::empty();
            if (regs.eax.b() & 0b0000_0001) != 0 {
                modifier |= Modifier::LEFT_SHIFT;
            }
            if (regs.eax.b() & 0b0000_1000) != 0 {
                modifier |= Modifier::LEFT_ALT
            }
            if (regs.eax.b() & 0b0001_0000) != 0 {
                modifier |= Modifier::LEFT_CTRL
            }

            drop(regs);
            let key_stroke = KeyStroke::new(usage, modifier);
            let ch = HidManager::translate(key_stroke).unwrap_or(0 as char);
            InputKey::new(key_stroke, ch as u16).into()
        }
    }
}

// Keyboard scan code to HID usage table
#[rustfmt::skip]
static SCAN_TO_HID: [u8; 128] = [
    /*         -0    -1    -2    -3    -4    -5    -6    -7    -8    -9    -A    -B    -C    -D    -E    -F */
    /* 0- */ 0x29, 0x1e, 0x1f, 0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x2d, 0x2e, 0x89, 0x2a, 0x2b,
    /* 1- */ 0x14, 0x1a, 0x08, 0x15, 0x17, 0x1c, 0x18, 0x0c, 0x12, 0x13, 0x2f, 0x30, 0x28, 0x04, 0x16, 0x07,
    /* 2- */ 0x09, 0x0a, 0x0b, 0x0d, 0x0e, 0x0f, 0x33, 0x34, 0x31, 0x1d, 0x1b, 0x06, 0x19, 0x05, 0x11, 0x10,
    /* 3- */ 0x36, 0x37, 0x38, 0x87, 0x2c, 0x8a, 0x4b, 0x4e, 0x49, 0x4c, 0x52, 0x50, 0x4f, 0x51, 0x4a, 0x4d,
    /* 4- */ 0x56, 0x54, 0x5f, 0x60, 0x61, 0x55, 0x5c, 0x5d, 0x5e, 0x57, 0x59, 0x5a, 0x5b, 0x67, 0x62, 0x85,
    /* 5- */ 0x63, 0x8b, 0x44, 0x45, 0x68, 0x69, 0x6a, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x29, 0x00,
    /* 6- */ 0x48, 0x46, 0x3a, 0x3b, 0x3c, 0x3d, 0x3e, 0x3f, 0x40, 0x41, 0x42, 0x43, 0x00, 0x00, 0x00, 0x00,
    /* 7- */ 0xe1, 0x39, 0x88, 0xe2, 0xe0, 0xe5, 0x00, 0xe3, 0xe7, 0x65, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];
