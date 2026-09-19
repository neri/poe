//! Platform dependent module for Arm virtual machine (QEMU virt)
//!
//! Devices are discovered from the device tree.
//! Requirements: GICv2 or GICv3, Generic Timer.
//! Optional: PL011 UART (the console is discarded without it),
//! framebuffer described in the coreboot table (the console is shown on it),
//! ChromeOS EC on Rockchip SPI (display backlight, keyboard as stdin in preference to UART,
//! with the layout from the VPD).

use core::arch::asm;
use core::ffi::c_void;
use core::time::Duration;

use fdt::PropName;

use super::{CurrentPlatform, MonotonicTimerPoller, Platform};
use crate::arch::gic::{Gic, Irq};
use crate::arch::gicv3::GicV3;
use crate::*;

/// Draws a boot progress marker in the diagnostic build (see `diag.rs`).
macro_rules! diag_mark {
    ($stage:expr) => {
        #[cfg(feature = "diag_rk3399")]
        #[allow(unused_unsafe)]
        unsafe {
            $crate::platform::virt::diag::mark($stage)
        };
    };
}

pub mod coreboot;
pub mod cros_ec;
pub mod cros_ec_keyb;
#[cfg(feature = "diag_rk3399")]
pub mod diag;
pub mod ec_packet;
pub mod fb;
pub mod keymatrix;
pub mod pl011;
pub mod rk_spi;
pub mod trap;
pub mod vop;
pub mod vpd;

unsafe extern "C" {
    unsafe static _end: c_void;
}

impl Platform for CurrentPlatform {
    unsafe fn init_dt_early(dt: &fdt::DeviceTree, _arg: usize) {
        diag_mark!(1);
        unsafe {
            // Without UART, stdin and stdout remain the null device.
            let uart_base = find_reg(dt, &["arm,pl011"], 0).map(|(base, _size)| base);
            if let Some(uart_base) = uart_base {
                pl011::Pl011::init(uart_base);
                System::set_stdin(pl011::Pl011::shared());
                System::set_stdout(pl011::Pl011::shared());
            }

            trap::init();
            diag_mark!(2);

            // depthcharge turns off the backlight when it blanks the screen before the handoff
            let backlight = cros_ec::CrosEc::find(dt)
                .map(|ec| ec.set_display_backlight(cros_ec::DEFAULT_BACKLIGHT));
            if let Some(Err(_)) = backlight {
                diag_mark!(diag::STAGE_BACKLIGHT_FAILED);
            }

            let boot_info = System::boot_info_mut();
            let end = PhysicalAddress::new(&_end as *const _ as PhysicalAddressRepr);
            boot_info.start_conventional_memory = end.rounding_up_4k().as_repr() as u32;
            boot_info.conventional_memory_size = 0x40_0000;

            PSCI_CONDUIT = find_psci_conduit(dt);

            let gic_info = init_irq_controller(dt);
            diag_mark!(3);
            irq_enable(arch::gic::IRQ_CNTV);
            arch::timer::GenericTimer::init();
            diag_mark!(4);

            println!("-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-");
            {
                let currentel: usize;
                asm!("mrs {}, currentel", out(reg)currentel);
                println!("Current EL is EL{}", (currentel & 0xC) >> 2);

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
                println!("{}", gic_info);
            }
        }
    }

    unsafe fn init(_arg: usize) {
        diag_mark!(5);
        println!("-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-");

        unsafe {
            Hal::cpu().enable_interrupt();
            diag_mark!(6);

            if let Some(dt) = System::device_tree() {
                // The layout of the keyboard is decided from the VPD, the same way as ChromeOS
                let vpd = coreboot::find_vpd_ro(dt);
                let layout = vpd
                    .as_deref()
                    .map_or(vpd::KeyboardLayout::Us, vpd::keyboard_layout);
                println!(
                    "Keyboard layout: {:?} (VPD {})",
                    layout,
                    if vpd.is_some() { "found" } else { "not found" }
                );

                // The keyboard of Chromebooks is used in preference to UART
                if let Some(ec) = cros_ec::CrosEc::find(dt)
                    && cros_ec_keyb::CrosEcKeyboard::install(ec, layout)
                {
                    println!("Keyboard: ChromeOS EC");
                }

                let fb = coreboot::find_framebuffer(dt)
                    .map(|mut fb| {
                        // On Rockchip, the address is not in the coreboot table
                        if fb.base == 0 {
                            fb.base = vop::scanout_address(dt).unwrap_or(0);
                        }
                        fb
                    })
                    .filter(|fb| fb.mode_info().is_some());
                diag_mark!(match fb {
                    Some(_) => diag::STAGE_FB_FOUND,
                    None => diag::STAGE_FB_NOT_FOUND,
                });
                if let Some(fb) = fb {
                    println!(
                        "Framebuffer: {:08x} {}x{} stride {} {}bpp",
                        fb.base, fb.width, fb.height, fb.stride, fb.bits_per_pixel
                    );
                    // The boot loader may have left lines of the framebuffer in the data cache
                    arch::cache::dcache_clean_invalidate(fb.base, fb.size());
                    // The firmware screen may have been stopped at the handoff
                    if let Some(vop) = vop::wake_up(dt) {
                        println!(
                            "VOP: {:08x} standby {} scanout {:08x}",
                            vop.base, vop.was_standby, vop.scanout
                        );
                    }
                    GRAPHICS_AVAILABLE = fb::FirmwareFb::install(&fb);
                }
            }
        }
    }

    unsafe fn exit() {
        // to do nothing for now
    }

    fn reset_system() -> ! {
        unsafe {
            psci_call(PSCI_SYSTEM_RESET);
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

/// Whether the framebuffer set up by the firmware is registered as the graphics output device
static mut GRAPHICS_AVAILABLE: bool = false;

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

/// Returns the `index`-th `reg` (base, size) of the first top-level node compatible with `compatible`.
fn find_reg(dt: &fdt::DeviceTree, compatible: &[&str], index: usize) -> Option<(usize, usize)> {
    for node in dt.root().children() {
        if node.status_is_ok() && compatible.iter().any(|v| node.is_compatible_with(v)) {
            return node
                .reg()?
                .nth(index)
                .map(|(base, size)| (base as usize, size as usize));
        }
    }
    None
}

static mut IRQ_CONTROLLER: IrqController = IrqController::GicV2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IrqController {
    GicV2,
    GicV3,
}

/// Initialize GICv3 or GICv2, whichever is found in the device tree.
///
/// Returns a description of the controller for the boot message.
unsafe fn init_irq_controller(dt: &fdt::DeviceTree) -> heapless::String<64> {
    const GICV2: &[&str] = &["arm,cortex-a15-gic", "arm,gic-400"];
    const GICV3: &[&str] = &["arm,gic-v3"];

    let mut info = heapless::String::new();
    unsafe {
        if let (Some((gicd_base, _)), Some((gicr_base, gicr_size))) =
            (find_reg(dt, GICV3, 0), find_reg(dt, GICV3, 1))
        {
            GicV3::init(gicd_base, gicr_base, gicr_size).expect("GICv3: initialization failed");
            IRQ_CONTROLLER = IrqController::GicV3;
            let _ = write!(
                info,
                "GICv3: GICD: {:08x} GICR: {:08x}",
                gicd_base, gicr_base
            );
        } else if let (Some((gicd_base, _)), Some((gicc_base, _))) =
            (find_reg(dt, GICV2, 0), find_reg(dt, GICV2, 1))
        {
            Gic::init(gicc_base, gicd_base);
            IRQ_CONTROLLER = IrqController::GicV2;
            let _ = write!(
                info,
                "GICv2: GICD: {:08x} GICC: {:08x}",
                gicd_base, gicc_base
            );
        } else {
            panic!("GIC not found");
        }
    }
    info
}

unsafe fn irq_enable(irq: Irq) {
    unsafe {
        match IRQ_CONTROLLER {
            IrqController::GicV2 => Gic::enable(irq),
            IrqController::GicV3 => GicV3::enable(irq),
        }
    }
}

/// Acknowledge the pending interrupt. An ID of 1020 or greater means no interrupt.
pub(super) unsafe fn irq_ack() -> Irq {
    unsafe {
        match IRQ_CONTROLLER {
            IrqController::GicV2 => Gic::ack(),
            IrqController::GicV3 => GicV3::ack(),
        }
    }
}

pub(super) unsafe fn irq_eoi(irq: Irq) {
    unsafe {
        match IRQ_CONTROLLER {
            IrqController::GicV2 => Gic::eoi(irq),
            IrqController::GicV3 => GicV3::eoi(irq),
        }
    }
}

const PSCI_SYSTEM_RESET: u32 = 0x8400_0009;

static mut PSCI_CONDUIT: PsciConduit = PsciConduit::None;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PsciConduit {
    None,
    Hvc,
    Smc,
}

fn find_psci_conduit(dt: &fdt::DeviceTree) -> PsciConduit {
    for node in dt.root().children() {
        if node.is_compatible_with("arm,psci-0.2") {
            return match node.get_prop_str(PropName::new("method")) {
                Some("hvc") => PsciConduit::Hvc,
                Some("smc") => PsciConduit::Smc,
                _ => PsciConduit::None,
            };
        }
    }
    PsciConduit::None
}

unsafe fn psci_call(function_id: u32) {
    unsafe {
        match PSCI_CONDUIT {
            PsciConduit::Hvc => {
                asm!("hvc #0", inlateout("x0") function_id as usize => _, clobber_abi("C"))
            }
            PsciConduit::Smc => {
                asm!("smc #0", inlateout("x0") function_id as usize => _, clobber_abi("C"))
            }
            PsciConduit::None => {}
        }
    }
}
