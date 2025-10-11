//! NEC PC-98 Series Computer
//!
//! # NOTE
//!
//! May not work or may need to be adjusted as it has not been fully verified on actual hardware.
//!

pub mod bios;
pub mod pc98_text;

use crate::arch::vm86::{VM86, X86StackContext};
use crate::mem::{MemoryManager, MemoryType};
use crate::platform::x86_pc::pic::Irq;
use crate::*;
use x86::isolated_io::LoIoPortDummyB;

pub static PORT_5F: LoIoPortDummyB<0x5F> = LoIoPortDummyB::new();

pub(super) unsafe fn init(_info: &BootInfo) {
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

        super::pit::Pit::init(0x0071, 0x3fdb, 0x0077, 2457, Irq(0), timer_irq_handler);
        Hal::cpu().enable_interrupt();

        let kbd = &mut *(&raw mut STDIN);
        kbd.reset();
        System::set_stdin(kbd);
    }
}

pub(super) unsafe fn exit() {
    // TODO:
}

fn timer_irq_handler(_irq: Irq) {
    super::pit::Pit::advance_tick();
}

static mut STDIN: BiosTextInput = BiosTextInput {};

struct BiosTextInput;

impl SimpleTextInput for BiosTextInput {
    fn reset(&mut self) {
        while self.read_key_stroke().is_some() {}
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
            InputKey {
                scan_code: regs.eax.h() as u16,
                unicode_char: regs.eax.b() as u16,
            }
            .into()
        }
    }
}
