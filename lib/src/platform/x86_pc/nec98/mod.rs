//! NEC PC-98 Series Computer
//!
//! # NOTE
//!
//! May not work or may need to be adjusted as it has not been fully verified on actual hardware.
//!

mod pc98_text;

use crate::arch::{cpu::X86StackContext, vm86::VM86};
use crate::*;
use mem::{MemoryManager, MemoryType};
use x86::prot::InterruptVector;

pub(super) unsafe fn init_early() {
    unsafe {
        pc98_text::Pc98Text::init();

        // let info = Environment::boot_info();
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
    }
}

pub(super) unsafe fn init_late() {
    unsafe {
        System::set_stdin(&mut *(&raw mut STDIN));
    }
}

pub(super) unsafe fn exit() {
    // TODO:
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
            regs.eax = 0x0100;
            VM86::call_bios(InterruptVector(0x18), &mut regs);
            if regs.bh() == 0 {
                None
            } else {
                regs.eax = 0;
                VM86::call_bios(InterruptVector(0x18), &mut regs);
                InputKey {
                    usage: (regs.eax >> 8) as u16,
                    unicode_char: (regs.eax & 0xFF) as u16,
                }
                .into()
            }
        }
    }
}
