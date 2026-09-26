//! IRQ notification for the QEMU virtio-mmio slots. The queue is still
//! reclaimed by the foreground owner; interrupt context only acknowledges
//! the transport and wakes a waiting request.
use core::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

#[cfg(any(all(target_arch = "aarch64", feature = "arm64dt"), feature = "sbi"))]
use super::mmio::Mmio;

struct Slot {
    base: AtomicUsize,
    irq: AtomicU32,
    events: AtomicU32,
    config_events: AtomicU32,
}

impl Slot {
    const fn new() -> Self {
        Self {
            base: AtomicUsize::new(0),
            irq: AtomicU32::new(0),
            events: AtomicU32::new(0),
            config_events: AtomicU32::new(0),
        }
    }
}

static SLOTS: [Slot; 32] = [const { Slot::new() }; 32];

pub fn number(node: &fdt::Node<'_>) -> Option<(u32, bool)> {
    let words = node.get_prop(fdt::PropName::INTERRUPTS)?.words();
    #[cfg(all(target_arch = "aarch64", feature = "arm64dt"))]
    {
        if words.len() == 3 && words[0].as_u32() == 0 {
            let edge = match words[2].as_u32() & 0xf {
                1 => true,
                4 => false,
                _ => return None,
            };
            return Some((words[1].as_u32().checked_add(32)?, edge));
        }
    }
    #[cfg(feature = "sbi")]
    {
        if words.len() == 1 {
            return Some((words[0].as_u32(), false));
        }
    }
    let _ = words;
    None
}

/// Installs an IRQ for a successfully identified modern MMIO device.
pub fn register(base: usize, irq: u32, edge: bool) -> bool {
    #[cfg(not(all(target_arch = "aarch64", feature = "arm64dt")))]
    let _ = edge;
    if base == 0 || irq == 0 {
        return false;
    }
    let Some(slot) = SLOTS
        .iter()
        .find(|slot| slot.base.load(Ordering::Acquire) == 0)
    else {
        return false;
    };
    slot.irq.store(irq, Ordering::Relaxed);
    slot.base.store(base, Ordering::Release);

    #[cfg(all(target_arch = "aarch64", feature = "arm64dt"))]
    let registered = unsafe {
        crate::platform::arm64dt::irq::register_handler_trigger(
            crate::arch::gic::Irq(irq),
            handle,
            edge,
        )
        .is_ok()
    };
    #[cfg(feature = "sbi")]
    let registered = crate::platform::rv_sbi::plic::register_source(irq);
    #[cfg(not(any(all(target_arch = "aarch64", feature = "arm64dt"), feature = "sbi")))]
    let registered = false;

    if !registered {
        slot.base.store(0, Ordering::Release);
    }
    registered
}

#[cfg(all(target_arch = "aarch64", feature = "arm64dt"))]
fn handle() {
    for slot in &SLOTS {
        let base = slot.base.load(Ordering::Acquire);
        if base == 0 {
            continue;
        }
        // The range was validated at discovery and remains mapped for boot.
        let mmio = unsafe { Mmio::new(base, 0x200) };
        if let Ok(mmio) = mmio {
            let status = mmio.ack_interrupt();
            if status != 0 {
                slot.events.fetch_add(1, Ordering::Release);
                if status & 2 != 0 {
                    slot.config_events.fetch_add(1, Ordering::Release);
                }
            }
        }
    }
}

#[cfg(feature = "sbi")]
pub fn handle_source(source: u32) {
    for slot in &SLOTS {
        if slot.irq.load(Ordering::Relaxed) != source {
            continue;
        }
        let base = slot.base.load(Ordering::Acquire);
        if base != 0
            && let Ok(mmio) = unsafe { Mmio::new(base, 0x200) }
        {
            let status = mmio.ack_interrupt();
            if status != 0 {
                slot.events.fetch_add(1, Ordering::Release);
                if status & 2 != 0 {
                    slot.config_events.fetch_add(1, Ordering::Release);
                }
            }
        }
        break;
    }
}

/// Sleeps only when an interrupt is registered and globally enabled. The
/// periodic timer also bounds a completion that races with WFI.
pub fn wait(base: usize) -> bool {
    if base == 0 {
        return false;
    }
    if !SLOTS
        .iter()
        .any(|slot| slot.base.load(Ordering::Acquire) == base)
    {
        return false;
    }
    use crate::arch::hal::{Hal, HalCpu, HalTrait};
    if !Hal::cpu().is_interrupt_enabled() {
        return false;
    }
    Hal::cpu().wait_for_interrupt();
    true
}

pub fn events(base: usize) -> u32 {
    if base == 0 {
        return 0;
    }
    SLOTS
        .iter()
        .find(|slot| slot.base.load(Ordering::Acquire) == base)
        .map_or(0, |slot| slot.events.load(Ordering::Acquire))
}

pub fn config_events(base: usize) -> u32 {
    if base == 0 {
        return 0;
    }
    SLOTS
        .iter()
        .find(|slot| slot.base.load(Ordering::Acquire) == base)
        .map_or(0, |slot| slot.config_events.load(Ordering::Acquire))
}

pub fn note_config(base: usize) {
    if base != 0
        && let Some(slot) = SLOTS
            .iter()
            .find(|slot| slot.base.load(Ordering::Acquire) == base)
    {
        slot.config_events.fetch_add(1, Ordering::Release);
    }
}

pub fn total_events() -> u32 {
    SLOTS
        .iter()
        .map(|slot| slot.events.load(Ordering::Acquire))
        .sum()
}
