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
use x86::isolated_io::{IoPortWB, LoIoPortRB, LoIoPortWB};

impl PlatformTrait for Platform {
    unsafe fn init(_arg: usize) {
        unsafe {
            let info = System::boot_info();

            cpu::Cpu::init();
            lomem::LoMemoryManager::init();

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
                    IoPortWB(0x0CF9).write(0x06);

                    // OADG reset
                    LoIoPortWB::<0x92>::new().write(0x01);

                    // PS/2 reset
                    loop {
                        let al = LoIoPortRB::<0x64>::new().read();
                        if (al & 0x02) == 0 {
                            break;
                        }
                    }
                    LoIoPortWB::<0x64>::new().write(0xfe);
                }
                Platform::Nec98 => {
                    LoIoPortWB::<0x37>::new().write(0x0f);
                    LoIoPortWB::<0x37>::new().write(0x0b);
                    LoIoPortWB::<0xf0>::new().write(0x00);
                }
                Platform::FmTowns => {
                    LoIoPortWB::<0x20>::new().write(0x01);
                    LoIoPortWB::<0x22>::new().write(0x00);
                }
                _ => unreachable!(),
            }

            Hal::cpu().stop();
        }
    }
}
