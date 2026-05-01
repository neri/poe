//! Platform dependent module for Raspberry Pi series

use super::{Platform, PlatformTrait};
use crate::*;
use core::{
    arch::{asm, naked_asm},
    cell::UnsafeCell,
    mem::MaybeUninit,
    sync::atomic::{Ordering, compiler_fence},
    time::Duration,
};

pub mod fb;
pub mod gpio;
pub mod mbox;
pub mod timer;
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
            println!("-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-");

            let boot_info = System::boot_info_mut();
            boot_info.platform = Platform::RaspberryPi;

            let _end: u64;
            asm!("ldr {}, =_end", out(reg)_end);
            let _end = PhysicalAddress::new(_end);
            boot_info.start_conventional_memory = _end.rounding_up_4k().as_repr() as u32;
            boot_info.conventional_memory_size = 0x40_0000;

            {
                let currentel: usize;
                asm!("mrs {}, currentel", out(reg)currentel);
                println!("Current EL is EL{}", (currentel & 0xC) >> 2);

                asm!(
                    "ldr {0}, =_vector_table",
                    "msr vbar_el1, {0}",
                    out(reg) _,
                );

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
        // TODO:
        println!("-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-");

        unsafe {
            fb::Fb::init();
        }
    }

    unsafe fn exit() {
        // Nothing to do
    }

    fn reset_system() -> ! {
        todo!()
    }

    fn monotonic() -> u64 {
        // TODO: implement
        0
    }

    fn duration_to_ticks(_duration: Duration) -> u64 {
        // TODO: implement
        0
    }
}

#[inline]
pub fn current_machine_type() -> MachineType {
    unsafe { CURRENT_MACHINE_TYPE.assume_init() }
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

#[unsafe(naked)]
#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.boot")]
#[allow(named_asm_labels)]
unsafe extern "C" fn _vector_table_nkf() {
    naked_asm!(
        ".align 11",
        "_vector_table:",
        "    // synchronous",
        "    sub sp, sp, #256",
        "    stp x0, x1, [sp, #16 * 0]",
        "    stp x2, x3, [sp, #16 * 1]",
        "    stp x4, x5, [sp, #16 * 2]",
        "    stp x6, x7, [sp, #16 * 3]",
        "    stp x8, x9, [sp, #16 * 4]",
        "    stp x10, x11, [sp, #16 * 5]",
        "    stp x12, x13, [sp, #16 * 6]",
        "    stp x14, x15, [sp, #16 * 7]",
        "    stp x16, x17, [sp, #16 * 8]",
        "    stp x18, x19, [sp, #16 * 9]",
        "    stp x20, x21, [sp, #16 * 10]",
        "    stp x22, x23, [sp, #16 * 11]",
        "    stp x24, x25, [sp, #16 * 12]",
        "    stp x26, x27, [sp, #16 * 13]",
        "    stp x28, x29, [sp, #16 * 14]",
        "    str x30, [sp, #16 * 15]",
        "    brk #1",
        "",
        ".align 7",
        "    // IRQ",
        "    brk #1",
        "",
        ".align 7",
        "    // FIQ",
        "    brk #1",
        "",
        ".align 7",
        "    // SError",
        "    brk #1",
        "",
        ".align 7",
        "_exc_handler:",
        "    brk #1",
        "    ldp x0, x1, [sp, #16 * 0]",
        "    ldp x2, x3, [sp, #16 * 1]",
        "    ldp x4, x5, [sp, #16 * 2]",
        "    ldp x6, x7, [sp, #16 * 3]",
        "    ldp x8, x9, [sp, #16 * 4]",
        "    ldp x10, x11, [sp, #16 * 5]",
        "    ldp x12, x13, [sp, #16 * 6]",
        "    ldp x14, x15, [sp, #16 * 7]",
        "    ldp x16, x17, [sp, #16 * 8]",
        "    ldp x18, x19, [sp, #16 * 9]",
        "    ldp x20, x21, [sp, #16 * 10]",
        "    ldp x22, x23, [sp, #16 * 11]",
        "    ldp x24, x25, [sp, #16 * 12]",
        "    ldp x26, x27, [sp, #16 * 13]",
        "    ldp x28, x29, [sp, #16 * 14]",
        "    ldr x30, [sp, #16 * 15] ",
        "    add sp, sp, #256",
        "    eret",
    );
}
