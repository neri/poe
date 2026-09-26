//! Polling VirtIO devices on QEMU Arm64 and RISC-V64 `virt`.
mod barrier;
pub mod block;
mod dma;
pub mod gpu;
pub(crate) mod irq;
mod mmio;
mod queue;
pub mod rng;
mod transport;

pub fn interrupt_count() -> u32 {
    irq::total_events()
}

use alloc::boxed::Box;
use alloc::sync::Arc;
use core::sync::atomic::{AtomicBool, Ordering};

use mmio::Mmio;
use queue::Queue;
use transport::Transport;

use crate::System;
#[cfg(not(any(
    all(target_arch = "aarch64", feature = "arm64dt"),
    all(target_arch = "riscv64", feature = "sbi")
)))]
use crate::platform::{CurrentPlatform, Platform};

#[cfg(all(target_arch = "riscv64", feature = "sbi"))]
static QEMU_RV_VIRT: AtomicBool = AtomicBool::new(false);

pub struct Device {
    base: usize,
    transport: Box<dyn Transport>,
    queue: Queue,
    features: u64,
    owner: Arc<AtomicBool>,
    config_seen: u32,
    config_dirty: bool,
}

impl Device {
    unsafe fn new(base: usize, size: usize, accepted: u64) -> Result<Self, &'static str> {
        let transport = Box::new(unsafe { Mmio::new(base, size) }?);
        let mut device = Self::from_transport(transport, accepted)?;
        device.base = base;
        Ok(device)
    }
    fn from_transport(transport: Box<dyn Transport>, accepted: u64) -> Result<Self, &'static str> {
        transport.set_status(0);
        let deadline = deadline(1_000_000);
        while transport.status() != 0 {
            if expired(deadline) {
                return Err("reset timeout");
            }
        }
        transport.set_status(1);
        transport.set_status(3);
        let offered = transport.features();
        if offered & (1 << 32) == 0 {
            transport.set_status(3 | 128);
            return Err("VIRTIO_F_VERSION_1 missing");
        }
        let features = offered & (accepted | (1 << 32));
        transport.set_features(features);
        transport.set_status(3 | 8);
        if transport.status() & (8 | 64 | 128) != 8 {
            transport.set_status(3 | 8 | 128);
            return Err("features rejected");
        }
        let owner = Arc::new(AtomicBool::new(false));
        let queue = match Queue::new(&*transport, 0, Some(owner.clone())) {
            Ok(queue) => queue,
            Err(error) => {
                transport.set_status(3 | 8 | 128);
                return Err(error);
            }
        };
        transport.set_status(3 | 8 | 4);
        if transport.status() & (4 | 64 | 128) != 4 {
            reset_transport(&*transport, &owner);
            return Err("driver failed");
        }
        Ok(Self {
            base: 0,
            transport,
            queue,
            features,
            owner,
            config_seen: 0,
            config_dirty: false,
        })
    }
    fn run(&mut self, buffers: &[queue::Buffer], timeout_us: u64) -> Result<u32, &'static str> {
        if self.owner.load(Ordering::Acquire) {
            return Err("device reset did not complete");
        }
        if self.transport.status() & (4 | 64 | 128) != 4 {
            return Err("device not ready");
        }
        self.queue.submit(&*self.transport, 0, buffers)?;
        let limit = deadline(timeout_us);
        loop {
            if self.transport.status() & (64 | 128) != 0 {
                self.reset();
                return Err("device needs reset");
            }
            let events = irq::events(self.base);
            let result = match self.queue.poll() {
                Ok(result) => result,
                Err(e) => {
                    self.reset();
                    return Err(e);
                }
            };
            if let Some(len) = result {
                if self.transport.ack_interrupt() & 2 != 0 {
                    self.config_dirty = true;
                    irq::note_config(self.base);
                }
                return Ok(len);
            }
            if expired(limit) {
                self.reset();
                return Err("request timeout");
            }
            // On QEMU RISC-V virt, multi-threaded TCG can leave WFI asleep
            // after a GPU completion. Poll only on that machine; other
            // platforms retain interrupt-driven waiting.
            #[cfg(all(target_arch = "riscv64", feature = "sbi"))]
            if QEMU_RV_VIRT.load(Ordering::Relaxed) {
                core::hint::spin_loop();
                continue;
            }
            if irq::events(self.base) == events && !irq::wait(self.base) {
                core::hint::spin_loop();
            }
        }
    }
    fn reset(&self) -> bool {
        reset_transport(&*self.transport, &self.owner)
    }
    fn restart(&mut self, accepted: u64) -> Result<(), &'static str> {
        if self.owner.load(Ordering::Acquire) || !self.reset() {
            return Err("reset timeout");
        }
        self.transport.set_status(1);
        self.transport.set_status(3);
        let offered = self.transport.features();
        if offered & (1 << 32) == 0 {
            self.transport.set_status(3 | 128);
            return Err("VIRTIO_F_VERSION_1 missing");
        }
        let features = offered & (accepted | (1 << 32));
        self.transport.set_features(features);
        self.transport.set_status(3 | 8);
        if self.transport.status() & (8 | 64 | 128) != 8 {
            self.transport.set_status(3 | 8 | 128);
            return Err("features rejected");
        }
        let queue = match Queue::new(&*self.transport, 0, Some(self.owner.clone())) {
            Ok(queue) => queue,
            Err(error) => {
                self.reset();
                return Err(error);
            }
        };
        self.queue = queue;
        self.transport.set_status(3 | 8 | 4);
        if self.transport.status() & (4 | 64 | 128) != 4 {
            self.reset();
            return Err("driver failed");
        }
        self.features = features;
        self.config_seen = irq::config_events(self.base);
        self.config_dirty = false;
        Ok(())
    }

    fn config_changed(&mut self) -> bool {
        if self.transport.ack_interrupt() & 2 != 0 {
            self.config_dirty = true;
            irq::note_config(self.base);
        }
        self.config_dirty || irq::config_events(self.base) != self.config_seen
    }
}

fn reset_transport(transport: &dyn Transport, owner: &Arc<AtomicBool>) -> bool {
    transport.set_status(0);
    let limit = deadline(1_000_000);
    while transport.status() != 0 && !expired(limit) {
        core::hint::spin_loop();
    }
    let done = transport.status() == 0;
    if !done {
        owner.store(true, Ordering::Release);
    }
    done
}

impl Drop for Device {
    fn drop(&mut self) {
        if !self.owner.load(Ordering::Acquire) {
            let _ = self.reset();
        }
    }
}

fn now_us() -> u64 {
    #[cfg(all(target_arch = "aarch64", feature = "arm64dt"))]
    {
        return crate::platform::arm64dt::counter_us();
    }
    #[cfg(all(target_arch = "riscv64", feature = "sbi"))]
    {
        return crate::platform::rv_sbi::timer::PlatformTimer::microseconds();
    }
    #[cfg(not(any(
        all(target_arch = "aarch64", feature = "arm64dt"),
        all(target_arch = "riscv64", feature = "sbi")
    )))]
    {
        CurrentPlatform::monotonic()
    }
}
fn deadline(us: u64) -> u64 {
    now_us().saturating_add(us)
}
fn expired(limit: u64) -> bool {
    now_us() >= limit
}

/// Enumerates at most 32 enabled DT nodes. Unsupported IDs are left untouched.
pub fn init(dt: &fdt::DeviceTree) {
    #[cfg(all(target_arch = "riscv64", feature = "sbi"))]
    {
        QEMU_RV_VIRT.store(dt.root().is_compatible_with("riscv-virtio"), Ordering::Relaxed);
        let _ = crate::platform::rv_sbi::plic::init(dt);
    }
    let mut count = 0;
    while count < 32 {
        let mut seen = 0;
        let found = fdt::bus::find_map(dt, |node, map| {
            if !node.is_compatible_with("virtio,mmio") {
                return None;
            }
            if seen != count {
                seen += 1;
                return None;
            }
            Some((map.reg(node, 0), irq::number(node)))
        });
        let Some((reg, interrupt)) = found else {
            break;
        };
        count += 1;
        let Some((base, size)) = reg else {
            continue;
        };
        if base > usize::MAX as u64 || size > usize::MAX as u64 {
            continue;
        }
        let mmio = match unsafe { Mmio::new(base as usize, size as usize) } {
            Ok(mmio) => mmio,
            Err(_) => continue,
        };
        if mmio.device_id() != 0 {
            crate::println!("virtio-mmio: {base:#x} id {}", mmio.device_id());
        }
        let (name, result) = match mmio.device_id() {
            4 => ("virtio-rng", rng::attach(base as usize, size as usize)),
            2 => ("virtio-blk", block::attach(base as usize, size as usize)),
            16 => ("virtio-gpu", gpu::attach(base as usize, size as usize)),
            _ => continue,
        };
        if let Err(e) = result {
            crate::println!("{name}: {e}");
        } else if let Some((interrupt, edge)) = interrupt
            && irq::register(base as usize, interrupt, edge)
        {
            crate::println!("{name}: IRQ {interrupt} enabled");
        }
    }
}

#[cfg(test)]
mod tests {
    use alloc::sync::Arc;
    use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};

    use super::*;

    struct FakeTransport {
        offered: u64,
        reject: bool,
        reject_driver: bool,
        fail_driver: bool,
        stuck_reset: bool,
        status: Arc<AtomicU32>,
        accepted: Arc<AtomicU64>,
        interrupt: Arc<AtomicU32>,
    }
    impl Transport for FakeTransport {
        fn status(&self) -> u32 {
            self.status.load(Ordering::SeqCst)
        }
        fn set_status(&self, value: u32) {
            if self.stuck_reset && value == 0 && self.status.load(Ordering::SeqCst) != 0 {
                return;
            }
            self.status.store(
                if self.reject {
                    value & !8
                } else if self.reject_driver {
                    value & !4
                } else if self.fail_driver && value & 4 != 0 {
                    value | 128
                } else {
                    value
                },
                Ordering::SeqCst,
            );
        }
        fn features(&self) -> u64 {
            self.offered
        }
        fn set_features(&self, value: u64) {
            self.accepted.store(value, Ordering::SeqCst);
        }
        fn select_queue(&self, _: u16) {}
        fn queue_max(&self) -> u32 {
            8
        }
        fn queue_ready(&self) -> bool {
            false
        }
        fn setup_queue(&self, _: u16, _: u64, _: u64, _: u64) {}
        fn notify(&self, _: u16) {}
        fn ack_interrupt(&self) -> u32 {
            self.interrupt.swap(0, Ordering::SeqCst)
        }
        fn config_u32(&self, _: usize) -> u32 {
            0
        }
        fn config_generation(&self) -> u32 {
            0
        }
    }
    fn fake(offered: u64, reject: bool) -> (Box<dyn Transport>, Arc<AtomicU32>, Arc<AtomicU64>) {
        let status = Arc::new(AtomicU32::new(0));
        let accepted = Arc::new(AtomicU64::new(0));
        (
            Box::new(FakeTransport {
                offered,
                reject,
                reject_driver: false,
                fail_driver: false,
                stuck_reset: false,
                status: status.clone(),
                accepted: accepted.clone(),
                interrupt: Arc::new(AtomicU32::new(0)),
            }),
            status,
            accepted,
        )
    }
    #[test]
    fn negotiates_only_supported_features() {
        let (transport, status, accepted) = fake((1 << 32) | (1 << 5) | (1 << 23), false);
        let device = Device::from_transport(transport, 1 << 5).ok().unwrap();
        assert_eq!(device.features, (1 << 32) | (1 << 5));
        assert_eq!(accepted.load(Ordering::SeqCst), device.features);
        assert_eq!(status.load(Ordering::SeqCst), 15);
        drop(device);
        assert_eq!(status.load(Ordering::SeqCst), 0);
    }
    #[test]
    fn rejects_missing_version_and_feature_refusal() {
        let (transport, status, _) = fake(1 << 5, false);
        assert!(matches!(
            Device::from_transport(transport, 1 << 5),
            Err("VIRTIO_F_VERSION_1 missing")
        ));
        assert_ne!(status.load(Ordering::SeqCst) & 128, 0);
        let (transport, status, _) = fake(1 << 32, true);
        assert!(matches!(
            Device::from_transport(transport, 0),
            Err("features rejected")
        ));
        assert_ne!(status.load(Ordering::SeqCst) & 128, 0);
    }
    #[test]
    fn failed_reset_quarantines_inflight_dma() {
        let status = Arc::new(AtomicU32::new(0));
        let accepted = Arc::new(AtomicU64::new(0));
        let transport = Box::new(FakeTransport {
            offered: 1 << 32,
            reject: false,
            reject_driver: false,
            fail_driver: false,
            stuck_reset: true,
            status,
            accepted,
            interrupt: Arc::new(AtomicU32::new(0)),
        });
        let mut device = Device::from_transport(transport, 0).ok().unwrap();
        let dma = dma::Dma::new(16, 16)
            .unwrap()
            .with_owner(device.owner.clone());
        let result = device.run(
            &[queue::Buffer {
                address: dma.addr(),
                length: 16,
                writable: true,
            }],
            0,
        );
        assert_eq!(result, Err("request timeout"));
        assert!(device.owner.load(Ordering::Acquire));
        assert_eq!(device.restart(0), Err("reset timeout"));
    }

    #[test]
    fn driver_ok_refusal_resets_before_dropping_queue() {
        let status = Arc::new(AtomicU32::new(0));
        let transport = Box::new(FakeTransport {
            offered: 1 << 32,
            reject: false,
            reject_driver: true,
            fail_driver: false,
            stuck_reset: false,
            status: status.clone(),
            accepted: Arc::new(AtomicU64::new(0)),
            interrupt: Arc::new(AtomicU32::new(0)),
        });
        assert!(matches!(
            Device::from_transport(transport, 0),
            Err("driver failed")
        ));
        assert_eq!(status.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn requests_do_not_use_queue_when_device_needs_reset() {
        let (transport, status, _) = fake(1 << 32, false);
        let mut device = Device::from_transport(transport, 0).unwrap();
        status.store(64, Ordering::SeqCst);
        assert_eq!(device.run(&[], 0), Err("device not ready"));
    }

    #[test]
    fn driver_failed_bit_is_rejected_and_reset() {
        let status = Arc::new(AtomicU32::new(0));
        let transport = Box::new(FakeTransport {
            offered: 1 << 32,
            reject: false,
            reject_driver: false,
            fail_driver: true,
            stuck_reset: false,
            status: status.clone(),
            accepted: Arc::new(AtomicU64::new(0)),
            interrupt: Arc::new(AtomicU32::new(0)),
        });
        assert!(matches!(
            Device::from_transport(transport, 0),
            Err("driver failed")
        ));
        assert_eq!(status.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn config_change_is_reported_until_restart() {
        let interrupt = Arc::new(AtomicU32::new(2));
        let transport = Box::new(FakeTransport {
            offered: 1 << 32,
            reject: false,
            reject_driver: false,
            fail_driver: false,
            stuck_reset: false,
            status: Arc::new(AtomicU32::new(0)),
            accepted: Arc::new(AtomicU64::new(0)),
            interrupt,
        });
        let mut device = Device::from_transport(transport, 0).unwrap();
        assert!(device.config_changed());
        assert!(device.config_changed());
        device.restart(0).unwrap();
        assert!(!device.config_changed());
    }
}
