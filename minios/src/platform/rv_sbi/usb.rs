//! Polling PCI xHCI attachment for RISC-V. QEMU virt probes an ECAM host
//! here; board-specific hosts call [`start_xhci`] after their own setup.

use alloc::alloc::{alloc_zeroed, dealloc};
use alloc::boxed::Box;
use core::alloc::Layout;
use core::ptr::NonNull;

use super::timer::PlatformTimer;
use crate::io::pci::host::{self, DmaWindow, PciHost};
use crate::io::usb::input::UsbTextInputMux;
use crate::io::usb::xhci::Xhci;
use crate::io::usb::xhci::device::XhciUsb;
use crate::io::usb::xhci::env::{DmaAllocation, XhciEnv};
use crate::{System, println};

struct RiscvXhciEnv {
    dma: DmaWindow,
}

impl XhciEnv for RiscvXhciEnv {
    fn now_us(&self) -> u64 {
        PlatformTimer::microseconds()
    }

    fn alloc_dma(&self, len: usize, align: usize) -> Option<DmaAllocation> {
        let layout = Layout::from_size_align(len, align).ok()?;
        let cpu = NonNull::new(unsafe { alloc_zeroed(layout) })?;
        let address = cpu.as_ptr() as u64;
        let reachable = address >= self.dma.cpu_start
            && address
                .checked_add(len as u64)
                .is_some_and(|end| end <= self.dma.cpu_end);
        if !reachable {
            unsafe { dealloc(cpu.as_ptr(), layout) };
            return None;
        }
        Some(DmaAllocation {
            cpu,
            device: address.wrapping_add(self.dma.offset),
            len,
            align,
        })
    }

    unsafe fn free_dma(&self, allocation: DmaAllocation) {
        if let Ok(layout) = Layout::from_size_align(allocation.len, allocation.align) {
            unsafe { dealloc(allocation.cpu.as_ptr(), layout) };
        }
    }

    fn write_barrier(&self) {
        unsafe { core::arch::asm!("fence iorw, iorw", options(nostack)) };
    }

    fn read_barrier(&self) {
        unsafe { core::arch::asm!("fence iorw, iorw", options(nostack)) };
    }
}

pub(super) unsafe fn init(tree: &fdt::DeviceTree) -> Result<(), &'static str> {
    let mut host = PciHost::probe_ecam(tree).ok_or("no ECAM PCI host")?;
    host.configure_bridges();
    let bdf = host
        .find_map(|host, bdf| {
            (host.class_of(bdf)
                == (
                    host::CLASS_SERIAL_USB,
                    host::SUBCLASS_USB,
                    host::PROG_IF_XHCI,
                ))
                .then_some(bdf)
        })
        .ok_or("no PCI xHCI device")?;
    let vendor = host.read_u16(bdf, host::config::VENDOR_ID);
    let device = host.read_u16(bdf, host::config::DEVICE_ID);
    let bar = host
        .setup_memory_bar(bdf, 0)
        .ok_or("xHCI BAR0 is outside PCI windows")?;
    host.enable_memory_and_bus_master(bdf);
    println!(
        "xHCI PCI: {} {:04x}:{:04x} BAR0 {:#x}",
        bdf, vendor, device, bar.cpu_address
    );

    start_xhci(bar.cpu_address, host.dma_window())
}

pub(super) fn start_xhci(bar: usize, dma: DmaWindow) -> Result<(), &'static str> {
    let env = Box::leak(Box::new(RiscvXhciEnv { dma }));
    let mut controller = unsafe { Xhci::new(bar, env) }.map_err(|error| {
        println!("xHCI reset/start error: {:?}", error);
        "xHCI reset/start failed"
    })?;
    controller.no_op().map_err(|error| {
        println!("xHCI No-Op error: {:?}", error);
        "xHCI command ring did not complete"
    })?;
    println!("xHCI: command ring ready, polling USB ports");

    crate::io::usb::class::msc::registry::with_global(|r| r.set_clock(PlatformTimer::microseconds));
    let mut usb = XhciUsb::new(controller);
    let deadline = PlatformTimer::microseconds().saturating_add(3_000_000);
    while !usb.keyboard_ready() && PlatformTimer::microseconds() < deadline {
        usb.poll_once().map_err(|_| "USB bootstrap poll failed")?;
        core::hint::spin_loop();
    }
    if usb.keyboard_ready() {
        println!("USB keyboard ready");
    }
    System::register_service(Box::new(usb)).map_err(|_| "USB service registration failed")?;
    let fallback = System::stdin();
    let mux = Box::leak(Box::new(UsbTextInputMux::new(fallback)));
    unsafe { System::set_stdin(mux) };
    Ok(())
}
