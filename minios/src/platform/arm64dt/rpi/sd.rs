//! Native SD slot setup for Raspberry Pi 3 and 4.

use super::{Gpio, MachineType, Mbox, Pull, Tag, mmio_base};
use crate::io::sd::sdhci::Sdhci;
use crate::io::sd::{self, BusWidth, DataDirection, Host, HostError, Response, ResponseType};
use crate::platform::arm64dt::counter_us;
use crate::{System, println};

const SDHCI_CLOCK_HZ: u32 = 50_000_000;

#[derive(Debug)]
pub enum InitError {
    UnsupportedBoard,
    MailboxBuffer,
    SetClock,
    InvalidClock,
    Card(sd::CardError),
    Registry,
}

/// Configure the board route and publish its SDHCI-backed card.
pub fn init() -> Result<(), InitError> {
    let (offset, clock, pins, released) = match super::current_machine_type() {
        MachineType::RaspberryPi3 => (
            0x0030_0000,
            super::mbox::ClockId::EMMC,
            [
                Gpio::Pin48,
                Gpio::Pin49,
                Gpio::Pin50,
                Gpio::Pin51,
                Gpio::Pin52,
                Gpio::Pin53,
            ],
            [
                Gpio::Pin34,
                Gpio::Pin35,
                Gpio::Pin36,
                Gpio::Pin37,
                Gpio::Pin38,
                Gpio::Pin39,
            ],
        ),
        MachineType::RaspberryPi4 => (
            0x0034_0000,
            super::mbox::ClockId::EMMC2,
            [
                Gpio::Pin34,
                Gpio::Pin35,
                Gpio::Pin36,
                Gpio::Pin37,
                Gpio::Pin38,
                Gpio::Pin39,
            ],
            [
                Gpio::Pin48,
                Gpio::Pin49,
                Gpio::Pin50,
                Gpio::Pin51,
                Gpio::Pin52,
                Gpio::Pin53,
            ],
        ),
        _ => return Err(InitError::UnsupportedBoard),
    };

    let mut mailbox = Mbox::PROP.fixed::<10>();
    let clock_index = mailbox
        .append(Tag::SetClockRate(clock, SDHCI_CLOCK_HZ, 0))
        .map_err(|_| InitError::MailboxBuffer)?;
    let mailbox = mailbox.call().map_err(|_| InitError::SetClock)?;
    let reported_clock = mailbox.response(clock_index + 1);
    // Firmware may clamp the requested rate (Pi 3 reports 200 MHz even
    // after requesting 50 MHz). Dividers must use the returned clock.
    if reported_clock == 0 {
        return Err(InitError::InvalidClock);
    }
    let pi4 = super::current_machine_type() == MachineType::RaspberryPi4;
    let label = if pi4 { "Pi4 SD" } else { "Pi3 SD" };
    if reported_clock != SDHCI_CLOCK_HZ {
        println!(
            "{}: mailbox clock id={} requested={} reported={} Hz",
            label, clock as u32, SDHCI_CLOCK_HZ, reported_clock
        );
    }

    for pin in released {
        pin.function(super::gpio::Function::INPUT);
    }
    for pin in pins {
        pin.init(Pull::UP, super::gpio::Function::ALT3);
    }

    let base = mmio_base() + offset;
    let inner = unsafe {
        if pi4 {
            Sdhci::new(base, reported_clock, counter_us)
        } else {
            Sdhci::new_bcm2835(base, reported_clock, counter_us)
        }
    };
    let host = RpiDiagnosticHost { inner, base, label };
    let index = sd::register(host).map_err(InitError::Card)?;
    let summary = sd::devices()
        .into_iter()
        .find(|device| device.index == index)
        .and_then(|device| device.info)
        .ok_or(InitError::Registry)?;
    println!(
        "SD: sd{} {:?} {} blocks, {}-bit at {} Hz",
        index,
        summary.kind,
        summary.capacity_blocks,
        match summary.bus_width {
            sd::BusWidth::One => 1,
            sd::BusWidth::Four => 4,
        },
        summary.clock_hz
    );
    Ok(())
}

/// Keep Raspberry Pi failure diagnostics local to these boards.
struct RpiDiagnosticHost {
    inner: Sdhci,
    base: usize,
    label: &'static str,
}

impl RpiDiagnosticHost {
    fn log(&self, phase: &str) {
        let read = |offset| unsafe { ((self.base + offset) as *const u32).read_volatile() };
        println!(
            "{}: {} base={:08x} present={:08x} host_power={:08x} clock_reset={:08x}",
            self.label,
            phase,
            self.base,
            read(0x24),
            read(0x28),
            read(0x2c)
        );
        println!(
            "{}: int={:08x} enable={:08x} signal={:08x} cmd_mode={:08x} arg={:08x} block={:08x} caps={:08x} ctrl2={:08x}",
            self.label,
            read(0x30),
            read(0x34),
            read(0x38),
            read(0x0c),
            read(0x08),
            read(0x04),
            read(0x40),
            read(0x3c)
        );
    }
}

impl Host for RpiDiagnosticHost {
    fn now_us(&self) -> u64 {
        self.inner.now_us()
    }
    fn reset(&mut self) -> Result<(), HostError> {
        let result = self.inner.reset();
        if result.is_err() {
            self.log("reset failed");
        }
        result
    }
    fn set_clock(&mut self, hz: u32) -> Result<u32, HostError> {
        let result = self.inner.set_clock(hz);
        if result.is_err() {
            println!("{}: set clock {} Hz -> {:?}", self.label, hz, result);
            self.log("clock failed");
        }
        result
    }
    fn set_bus_width(&mut self, width: BusWidth) -> Result<(), HostError> {
        self.inner.set_bus_width(width)
    }
    fn prepare_data(
        &mut self,
        direction: DataDirection,
        block_size: u16,
        blocks: u16,
    ) -> Result<(), HostError> {
        self.inner.prepare_data(direction, block_size, blocks)
    }
    fn command(
        &mut self,
        index: u8,
        argument: u32,
        response: ResponseType,
        deadline_us: u64,
    ) -> Result<Response, HostError> {
        let result = self.inner.command(index, argument, response, deadline_us);
        if result.is_err() {
            println!(
                "{}: CMD{} arg={:08x} -> {:?}",
                self.label, index, argument, result
            );
            self.log("after command");
        }
        result
    }
    fn read_data(&mut self, bytes: &mut [u8], deadline_us: u64) -> Result<(), HostError> {
        self.inner.read_data(bytes, deadline_us)
    }
    fn write_data(&mut self, bytes: &[u8], deadline_us: u64) -> Result<(), HostError> {
        self.inner.write_data(bytes, deadline_us)
    }
}
