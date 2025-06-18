pub mod fm_towns;
pub mod ibm_pc;
pub mod nec98;

mod pic;
mod pit;

use super::{Platform, PlatformTrait};
use crate::{
    arch::{cpu, lomem, vm86},
    mem::{MemoryManager, MemoryType},
    *,
};
use core::arch::asm;
use x86::isolated_io::IoPort;

impl PlatformTrait for Platform {
    unsafe fn init(_arg: usize) {
        unsafe {
            let info = System::boot_info();

            cpu::Cpu::init();
            lomem::LowMemoryManager::init();

            MemoryManager::register_memmap(
                0x10_0000..info.start_conventional_memory as u64,
                MemoryType::Used,
            )
            .unwrap();

            match info.platform {
                Platform::Nec98 => {
                    nec98::init_early();
                }
                Platform::PcBios => {
                    ibm_pc::init_early();
                }
                Platform::FmTowns => {
                    fm_towns::init_early();
                }
                _ => unreachable!(),
            }

            vm86::VM86::init();
            pic::Pic::init(info.platform);
            pit::Pit::init(info.platform);
            Hal::cpu().enable_interrupt();

            match info.platform {
                Platform::Nec98 => {
                    nec98::init_late();
                }
                Platform::PcBios => {
                    ibm_pc::init_late();
                }
                Platform::FmTowns => {
                    fm_towns::init_late();
                }
                _ => unreachable!(),
            }
        }
    }

    unsafe fn exit() {
        unsafe {
            let platform = System::platform();
            match platform {
                Platform::Nec98 => {
                    nec98::exit();
                }
                Platform::PcBios => {
                    ibm_pc::exit();
                }
                Platform::FmTowns => {
                    fm_towns::exit();
                }
                _ => unreachable!(),
            }
            pic::Pic::exit();
        }
    }

    fn reset_system() -> ! {
        unsafe {
            match System::platform() {
                Platform::PcBios => {
                    // PCI reset
                    IoPort(0x0CF9).out8(0x06);

                    // OADG reset
                    asm!("out 0x92, al", in("al") 0x01u8);

                    // PS/2 reset
                    loop {
                        let al: u8;
                        asm!("in al, 0x64", out("al") al);
                        if (al & 0x02) == 0 {
                            break;
                        }
                    }
                    asm!("out 0x64, al", in("al") 0xFEu8);
                }
                Platform::Nec98 => {
                    asm!("out 0x37, al", in("al") 0x0Fu8);
                    asm!("out 0x37, al", in("al") 0x0Bu8);
                    asm!("out 0xF0, al", in("al") 0x00u8);
                }
                Platform::FmTowns => {
                    asm!("out 0x20, al", in("al") 0x01u8);
                    asm!("out 0x22, al", in("al") 0x00u8);
                }
                _ => unreachable!(),
            }

            Hal::cpu().stop();
        }
    }
}
