//! Platform dependent module for Raspberry Pi series

use super::{Platform, PlatformTrait};
use crate::*;
use core::{
    arch::asm,
    cell::UnsafeCell,
    mem::MaybeUninit,
    sync::atomic::{Ordering, compiler_fence},
    time::Duration,
};

pub mod fb;
pub mod gpio;
pub mod mbox;
pub mod trap;
pub mod uart0;
pub mod uart1;

impl PlatformTrait for Platform {
    unsafe fn init_dt_early(dt: &fdt::DeviceTree, _arg: usize) {
        unsafe {
            // detect machine type
            let midr_el1: usize;
            asm!("mrs {}, midr_el1", out(reg) midr_el1);
            (&mut *(&raw mut CURRENT_MACHINE_TYPE)).write(match (midr_el1 >> 4) & 0xfff {
                // 0xb76 => // rpi1
                // 0xc07 => // rpi2
                0xd03 => MachineType::RaspberryPi3,
                0xd08 => MachineType::RaspberryPi4,
                0xd0b => MachineType::RaspberryPi5,
                _ => MachineType::Unknown,
            });

            let simple_bus = dt
                .root()
                .children()
                .find(|v| v.status_is_ok() && v.is_compatible_with("simple-bus"))
                .unwrap();
            for item in simple_bus.ranges().unwrap() {
                if item.child == 0x7e00_0000 {
                    set_mmio_base(item.parent as usize);
                    break;
                }
            }

            if mmio_base() == 0 {
                set_mmio_base(match current_machine_type() {
                    MachineType::Unknown => 0x00_2000_0000,
                    MachineType::RaspberryPi3 => 0x00_3f00_0000,
                    MachineType::RaspberryPi4 => 0x00_fe00_0000,
                    MachineType::RaspberryPi5 => 0x10_7c00_0000,
                });
            }

            uart0::Uart0::init().unwrap();
            System::set_stdin(uart0::Uart0::shared());
            System::set_stdout(uart0::Uart0::shared());
            System::set_stderr(uart0::Uart0::shared());

            trap::init();

            let boot_info = System::boot_info_mut();
            boot_info.platform = Platform::RaspberryPi;

            let _end: u64;
            asm!("ldr {}, =_end", out(reg)_end);
            let _end = PhysicalAddress::new(_end);
            boot_info.start_conventional_memory = _end.rounding_up_4k().as_repr() as u32;
            boot_info.conventional_memory_size = 0x40_0000;

            match current_machine_type() {
                MachineType::RaspberryPi3 => {
                    (0x4000_0040 as *mut u32).write_volatile(0b1000); // enable timer interrupt
                }
                MachineType::RaspberryPi4 => {
                    arch::gic::Gic::init(0xff84_2000, 0xff84_1000);
                    arch::gic::Gic::enable(arch::gic::Irq(27));
                }
                _ => unreachable!(),
            }
            arch::timer::GenericTimer::init();

            println!("-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-");
            {
                let currentel: usize;
                asm!("mrs {}, currentel", out(reg)currentel);
                println!("Current EL is EL{}", (currentel & 0xC) >> 2);

                println!(
                    "Machine Type: {:?} ({:03x})",
                    current_machine_type(),
                    (midr_el1 >> 4) & 0xfff,
                );
                println!("Model: {}", dt.root().model());
                for item in dt.root().compatible().unwrap() {
                    println!("compatible: {}", item);
                }
            }
        }
    }

    unsafe fn init(_arg: usize) {
        println!("-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-");

        unsafe {
            fb::Fb::init();

            Hal::cpu().enable_interrupt();
        }
    }

    unsafe fn exit() {
        // to do nothing for now
    }

    fn reset_system() -> ! {
        todo!()
    }

    #[inline]
    fn monotonic() -> u64 {
        arch::timer::GenericTimer::monotonic()
    }

    #[inline]
    fn duration_to_ticks(duration: Duration) -> u64 {
        arch::timer::GenericTimer::duration_to_ticks(duration)
    }
}

#[inline]
pub fn current_machine_type() -> MachineType {
    unsafe { CURRENT_MACHINE_TYPE.assume_init() }
}

pub unsafe fn timer_eoi() {
    unsafe {
        match current_machine_type() {
            MachineType::RaspberryPi3 => {
                // to do nothing for now
            }
            MachineType::RaspberryPi4 => {
                arch::gic::Gic::eoi(arch::gic::Irq(27));
            }
            _ => unreachable!(),
        }
    }
}

static mut CURRENT_MACHINE_TYPE: MaybeUninit<MachineType> = MaybeUninit::zeroed();

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum MachineType {
    #[default]
    Unknown,
    RaspberryPi3,
    RaspberryPi4,
    RaspberryPi5,
}

static mut MMIO_BASE: UnsafeCell<usize> = UnsafeCell::new(0);

#[inline]
fn mmio_base() -> usize {
    unsafe { *&*(&*(&raw const MMIO_BASE)).get() }
}

#[inline]
unsafe fn set_mmio_base(value: usize) {
    compiler_fence(Ordering::SeqCst);
    unsafe {
        *(&mut *(&raw mut MMIO_BASE)).get_mut() = value;
    }
    compiler_fence(Ordering::SeqCst);
}
