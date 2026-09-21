//! Raspberry Pi 3 / 4 (BCM2837 / BCM2711)
//!
//! Raspberry Pi specific parts: the peripheral base address, the pins and the clock of UART0
//! (PL011, driven by the common driver), the framebuffer through the VideoCore mailbox,
//! the local interrupt controller of Raspberry Pi 3, and the reset through the watchdog.

use core::cell::UnsafeCell;
use core::mem::MaybeUninit;
use core::sync::atomic::{Ordering, compiler_fence};

use gpio::{Gpio, Pull};
use mbox::{ClockId, Mbox, Tag};
use pm::Pm;

use super::dt;

pub mod armctrl;
#[cfg(feature = "usb")]
pub mod dwc2;
pub mod fb;
pub mod gpio;
pub mod local_intc;
pub mod mbox;
#[cfg(feature = "usb")]
pub mod pcie;
pub mod pm;
pub mod uart1;
#[cfg(feature = "usb")]
pub mod usb;

/// Bus address of the peripherals (`/soc`)
const PERIPHERAL_BUS_BASE: u64 = 0x7e00_0000;

/// Clock of UART0 set through the mailbox
pub const UART0_CLOCK: u32 = 3_000_000;
pub const UART0_BAUD_RATE: u32 = 115_200;

/// Detects Raspberry Pi from the device tree and sets the peripheral base address.
///
/// Returns `false` if the machine is not a supported Raspberry Pi (Raspberry Pi 5 is not supported).
pub unsafe fn init_early(dt: &fdt::DeviceTree) -> bool {
    let root = dt.root();
    let machine_type =
        if root.is_compatible_with("brcm,bcm2837") || root.is_compatible_with("brcm,bcm2710") {
            MachineType::RaspberryPi3
        } else if root.is_compatible_with("brcm,bcm2711") {
            MachineType::RaspberryPi4
        } else {
            return false;
        };

    unsafe {
        (&mut *(&raw mut CURRENT_MACHINE_TYPE)).write(machine_type);

        if let Some(base) = dt::translate_bus_address(dt, &["simple-bus"], PERIPHERAL_BUS_BASE) {
            set_mmio_base(base);
        }

        if mmio_base() == 0 {
            set_mmio_base(match machine_type {
                MachineType::RaspberryPi4 => 0x00_fe00_0000,
                _ => 0x00_3f00_0000,
            });
        }

        let pm_base = dt::find_reg(dt, &[Pm::COMPATIBLE], 0)
            .map(|(base, _size)| base)
            .unwrap_or(mmio_base() + Pm::DEFAULT_OFFSET);
        Pm::init(pm_base);
    }
    true
}

/// Routes UART0 (PL011) to GPIO 14/15 and sets its clock.
///
/// Returns the clock and the baud rate to be programmed into the PL011.
pub unsafe fn init_uart0() -> Option<(u32, u32)> {
    Gpio::UART0_TXD.use_as_alt0();
    Gpio::UART0_RXD.use_as_alt0();
    Gpio::enable_pins(&[Gpio::UART0_TXD, Gpio::UART0_RXD], Pull::NONE);

    let mut mbox = Mbox::PROP.fixed::<10>();
    mbox.append(Tag::SetClockRate(ClockId::UART, UART0_CLOCK, 0))
        .ok()?;
    mbox.call().ok()?;
    Some((UART0_CLOCK, UART0_BAUD_RATE))
}

/// Resets the system with the watchdog. Returns if the machine is not a supported Raspberry Pi.
pub unsafe fn reset_system() {
    unsafe {
        Pm::reset_system();
    }
}

/// Registers the framebuffer of the VideoCore as the graphics output device.
pub unsafe fn init_graphics() {
    unsafe {
        fb::Fb::init();
    }
}

/// Returns whether the machine is a supported Raspberry Pi (after [`init_early`]).
#[inline]
pub fn is_detected() -> bool {
    current_machine_type() != MachineType::Unknown
}

#[inline]
pub fn current_machine_type() -> MachineType {
    unsafe { CURRENT_MACHINE_TYPE.assume_init() }
}

static mut CURRENT_MACHINE_TYPE: MaybeUninit<MachineType> = MaybeUninit::zeroed();

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum MachineType {
    #[default]
    Unknown = 0,
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
