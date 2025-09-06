//! Platform dependent module for Raspberry Pi series

use super::{Platform, PlatformTrait};
use crate::{mem::MemoryManager, *};
use core::{
    arch::asm,
    cell::UnsafeCell,
    mem::MaybeUninit,
    sync::atomic::{Ordering, compiler_fence},
};
use fb::Fb;
use fdt::FdtNode;
use minilib::rand::*;

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
            boot_info.start_conventional_memory =
                _end.rounding_up(MemoryManager::PAGE_SIZE).as_repr() as u32;
            boot_info.conventional_memory_size = 0x40_0000;

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

            let _ = Fb::set_overscan(0, 0, 0, 0);
            let width;
            let height;
            let mut edid = [0; 128];
            if let Ok((x, y)) = Fb::get_edid_size(Some(&mut edid)) {
                width = x;
                height = y;

                println!("EDID: ");
                for (i, &v) in edid.iter().enumerate() {
                    print!(" {:02x}", v);
                    if (i & 15) == 15 {
                        println!("");
                    }
                }
            } else {
                (width, height) = Fb::get_default_size();
            }
            if let Ok((ptr, w, h, stride)) = Fb::init(width, height) {
                println!("Framebuffer: {:p} {}x{} {}", ptr, w, h, stride);

                let mut rng = XorShift32::default();
                for y in 0..h {
                    for x in 0..w {
                        ptr.add(y as usize * stride + x as usize)
                            .write_volatile(rng.next() as u32 & 0x00ff_ffff);
                    }
                }

                //     STD_SCR_PTR.store(ptr as usize, Ordering::Relaxed);
                //     STD_SCR_W.store(w, Ordering::Relaxed);
                //     STD_SCR_H.store(h, Ordering::Relaxed);
                //     STD_SCR_S.store(stride, Ordering::Relaxed);
            }
        }
    }

    unsafe fn init(_arg: usize) {
        // TODO:
        println!("-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-");
    }

    unsafe fn exit() {
        // TODO:
    }

    fn reset_system() -> ! {
        todo!()
    }
}

// #[inline]
// pub fn std_screen() -> Option<(*mut TrueColor, usize, usize, usize)> {
//     let ptr = STD_SCR_PTR.load(Ordering::Relaxed) as *mut TrueColor;
//     (ptr != null_mut()).then(|| {
//         (
//             ptr,
//             STD_SCR_W.load(Ordering::Relaxed),
//             STD_SCR_H.load(Ordering::Relaxed),
//             STD_SCR_S.load(Ordering::Relaxed),
//         )
//     })
// }

// static STD_SCR_PTR: AtomicUsize = AtomicUsize::new(0);
// static STD_SCR_W: AtomicUsize = AtomicUsize::new(0);
// static STD_SCR_H: AtomicUsize = AtomicUsize::new(0);
// static STD_SCR_S: AtomicUsize = AtomicUsize::new(0);

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
