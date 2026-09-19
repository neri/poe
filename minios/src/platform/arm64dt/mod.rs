//! Platform dependent module for 64-bit Arm machines with a device tree
//!
//! One kernel image supports the machines below. Devices are discovered from the device tree,
//! and the machine specific parts are used only when the device tree has them.
//!
//! - QEMU virt machine
//! - Raspberry Pi 3 / 4 (`rpi`: peripheral base, UART0 pins and clock, mailbox framebuffer,
//!   local interrupt controller of Raspberry Pi 3)
//! - Chromebooks (`cros`: coreboot table, VPD, ChromeOS EC) with RK3399 (`rk3399`: SPI, VOP)
//!
//! Requirements: GICv2, GICv3 or the local interrupt controller of Raspberry Pi 3, Generic Timer.
//! Optional: PL011 UART (the console is discarded without it),
//! framebuffer from the VideoCore mailbox or the coreboot table (the console is shown on it),
//! ChromeOS EC on SPI (display backlight, keyboard as stdin in preference to UART,
//! with the layout from the VPD).

use core::arch::asm;
use core::ffi::c_void;
use core::time::Duration;

use cros::cros_ec::CrosEc;
use rk3399::rk_spi::RkSpi;
use spi::SpiDevice;

use super::{CurrentPlatform, MonotonicTimerPoller, Platform};
use crate::*;

pub mod cros;
pub mod dt;
pub mod fb;
pub mod irq;
pub mod pl011;
pub mod psci;
pub mod rk3399;
pub mod rpi;
pub mod spi;
pub mod trap;

unsafe extern "C" {
    unsafe static _end: c_void;
}

impl Platform for CurrentPlatform {
    unsafe fn init_dt_early(dt: &fdt::DeviceTree, _arg: usize) {
        unsafe {
            let is_rpi = rpi::init_early(dt);

            // Without UART, stdin and stdout remain the null device.
            let uart_base = dt::find_reg(dt, &["arm,pl011"], 0).map(|(base, _size)| base);
            if let Some(uart_base) = uart_base {
                let baud_rate = if is_rpi { rpi::init_uart0() } else { None };
                pl011::Pl011::init(uart_base, baud_rate);
                System::set_stdin(pl011::Pl011::shared());
                System::set_stdout(pl011::Pl011::shared());
            }

            trap::init();

            // depthcharge turns off the backlight when it blanks the screen before the handoff
            let backlight = find_cros_ec(dt)
                .map(|ec| ec.set_display_backlight(cros::cros_ec::DEFAULT_BACKLIGHT));

            let boot_info = System::boot_info_mut();
            if is_rpi {
                boot_info.platform_type = PlatformType::RaspberryPi;
            }
            let end = PhysicalAddress::new(&_end as *const _ as PhysicalAddressRepr);
            boot_info.start_conventional_memory = end.rounding_up_4k().as_repr() as u32;
            boot_info.conventional_memory_size = 0x40_0000;

            psci::init(dt);

            let irq_info = irq::init(dt);
            irq::enable(arch::gic::IRQ_CNTV);
            arch::timer::GenericTimer::init();

            println!("-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-");
            {
                let currentel: usize;
                asm!("mrs {}, currentel", out(reg)currentel);
                println!("Current EL is EL{}", (currentel & 0xC) >> 2);

                if is_rpi {
                    println!("Machine Type: {:?}", rpi::current_machine_type());
                }
                println!("Model: {}", dt.root().model());
                for item in dt.root().compatible().unwrap() {
                    println!("compatible: {}", item);
                }
                println!("UART: {:08x}", uart_base.unwrap_or_default());
                match backlight {
                    Some(Ok(())) => println!("Backlight: on"),
                    Some(Err(err)) => println!("Backlight: EC error {:?}", err),
                    None => {}
                }
                println!("{}", irq_info);
            }
        }
    }

    unsafe fn init(_arg: usize) {
        println!("-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-");

        unsafe {
            Hal::cpu().enable_interrupt();

            if let Some(dt) = System::device_tree() {
                // The keyboard of Chromebooks is used in preference to UART
                if let Some(ec) = find_cros_ec(dt) {
                    cros::install_keyboard(dt, ec);
                }

                GRAPHICS_AVAILABLE = if rpi::is_detected() {
                    rpi::init_graphics();
                    true
                } else {
                    init_firmware_framebuffer(dt)
                };
            }
        }
    }

    unsafe fn exit() {
        // to do nothing for now
    }

    fn reset_system() -> ! {
        unsafe {
            psci::system_reset();
        }
        Self::halt();
    }

    fn recommended_console_mode() -> RecommendedConsoleMode {
        if unsafe { GRAPHICS_AVAILABLE } {
            RecommendedConsoleMode::Graphics
        } else {
            RecommendedConsoleMode::None
        }
    }

    #[inline]
    fn monotonic() -> u64 {
        arch::timer::GenericTimer::monotonic()
    }

    #[inline]
    fn create_timer_event(duration: Duration) -> Box<dyn PollingEvent> {
        let timeout = Self::monotonic() + arch::timer::GenericTimer::duration_to_ticks(duration);
        Box::new(MonotonicTimerPoller::Timeout(timeout))
    }
}

/// Whether a graphics output device is registered
static mut GRAPHICS_AVAILABLE: bool = false;

/// Finds the ChromeOS EC on a supported SPI controller.
fn find_cros_ec(dt: &fdt::DeviceTree) -> Option<CrosEc<impl SpiDevice + 'static>> {
    cros::cros_ec::find(dt, |controller, (base, _size), cs| {
        controller
            .is_compatible_with(RkSpi::COMPATIBLE)
            .then(|| RkSpi::new(base, cs))
    })
}

/// Registers the framebuffer set up by the firmware as the graphics output device.
///
/// Returns `false` if there is no framebuffer or its pixel format is not supported.
unsafe fn init_firmware_framebuffer(dt: &fdt::DeviceTree) -> bool {
    let fb = cros::coreboot::find_framebuffer(dt)
        .map(|mut fb| {
            // On Rockchip, the address is not in the coreboot table
            if fb.base == 0 {
                fb.base = unsafe { rk3399::vop::scanout_address(dt) }.unwrap_or(0);
            }
            fb
        })
        .filter(|fb| fb.mode_info().is_some());
    let Some(fb) = fb else {
        return false;
    };
    println!(
        "Framebuffer: {:08x} {}x{} stride {} {}bpp",
        fb.base, fb.width, fb.height, fb.stride, fb.bits_per_pixel
    );
    unsafe {
        // The boot loader may have left lines of the framebuffer in the data cache
        arch::cache::dcache_clean_invalidate(fb.base, fb.size());
        // The firmware screen may have been stopped at the handoff
        if let Some(vop) = rk3399::vop::wake_up(dt) {
            println!(
                "VOP: {:08x} standby {} scanout {:08x}",
                vop.base, vop.was_standby, vop.scanout
            );
        }
    }
    fb::FirmwareFb::install(&fb)
}

/// Makes the device tree blob written by the boot loader visible with the data cache off.
///
/// depthcharge writes the DTB with the data cache on and turns the cache off before jumping
/// to the OS, so the DTB may still be only in the cache.
/// (Linux does not notice, since it reads the DTB after turning its own cache on.)
/// Call this before parsing the DTB.
pub unsafe fn clean_dtb_cache(dtb: usize) {
    if dtb == 0 || (dtb & 3) != 0 {
        return;
    }
    unsafe {
        arch::cache::dcache_clean_invalidate(dtb, 64);
        let totalsize = u32::from_be(((dtb + 4) as *const u32).read_volatile()) as usize;
        if totalsize > 64 && totalsize <= MAX_DTB_SIZE {
            arch::cache::dcache_clean_invalidate(dtb, totalsize);
        }
    }
}

const MAX_DTB_SIZE: usize = 0x20_0000;

/// Microseconds from the counter, available without the timer interrupt
fn counter_us() -> u64 {
    let cntvct: u64;
    unsafe {
        asm!("isb", "mrs {}, cntvct_el0", out(reg) cntvct);
    }
    let freq = arch::timer::GenericTimer::counter_freq().max(1) as u128;
    (cntvct as u128 * 1_000_000 / freq) as u64
}

fn delay_us(us: u64) {
    let start = counter_us();
    while counter_us().wrapping_sub(start) < us {
        core::hint::spin_loop();
    }
}
