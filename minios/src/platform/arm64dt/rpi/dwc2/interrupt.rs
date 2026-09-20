//! DWC2 interrupt handler.
//!
//! POE runs with the MMU off, so every access is to Device memory, where the
//! exclusive instructions behind an atomic read-modify-write are not supported
//! and abort. Everything here is therefore plain loads and stores: each
//! location has exactly one writer, and where the foreground has to read and
//! clear one it masks interrupts for those two instructions instead.

use core::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};

use super::super::armctrl::Armctrl;
use super::regs;
use crate::arch::gic::Irq;
use crate::arch::hal::{Hal, HalCpu, HalTrait};

static BASE: AtomicUsize = AtomicUsize::new(0);
static IRQ: AtomicU32 = AtomicU32::new(0);
/// Written by the handler only.
pub static EVENT_GENERATION: AtomicU32 = AtomicU32::new(0);
pub static PORT_EVENTS: AtomicU32 = AtomicU32::new(0);
pub static CHANNEL_EVENTS: [AtomicU32; 16] = [const { AtomicU32::new(0) }; 16];
pub static LAST_GINTSTS: AtomicU32 = AtomicU32::new(0);

/// Entries into the handler, and the value the foreground last saw.
static ENTRIES: AtomicU32 = AtomicU32::new(0);
static OBSERVED: AtomicU32 = AtomicU32::new(0);
static STORMED: AtomicBool = AtomicBool::new(false);

/// Handler entries without the foreground running once in between. The
/// foreground polls on every system tick, so anything beyond a small burst
/// means the interrupt is being re-taken faster than it can be serviced.
const STORM_LIMIT: u32 = 1024;

/// Points the ISR at the controller. Until this is called the handler does
/// nothing and completions are reaped by polling `HCINT` in the foreground.
pub fn install(base: usize, irq: Irq) {
    IRQ.store(irq.0, Ordering::Relaxed);
    BASE.store(base, Ordering::Release)
}

/// True while completions can be expected to arrive as interrupts.
pub fn is_active() -> bool {
    BASE.load(Ordering::Acquire) != 0 && !STORMED.load(Ordering::Relaxed)
}

/// True once the handler gave the interrupt up as unserviceable.
pub fn stormed() -> bool {
    STORMED.load(Ordering::Relaxed)
}

/// Called from the foreground to show that it is still getting time.
pub fn note_progress() {
    OBSERVED.store(ENTRIES.load(Ordering::Relaxed), Ordering::Relaxed)
}

/// Takes what the handler published for one channel.
///
/// The handler is the other writer, so the read and the clear are done with
/// interrupts masked; an interrupt landing between them would otherwise lose
/// a completion.
pub fn take_channel_events(channel: usize) -> u32 {
    let _guard = unsafe { Hal::cpu().interrupt_guard() };
    let events = CHANNEL_EVENTS[channel].load(Ordering::Acquire);
    if events != 0 {
        CHANNEL_EVENTS[channel].store(0, Ordering::Relaxed);
    }
    events
}

/// Minimal ISR: snapshot channel/port state, acknowledge W1C bits, and publish.
pub fn handle() {
    let base = BASE.load(Ordering::Acquire);
    if base == 0 {
        return;
    }
    // Interrupts are masked for the length of this handler, so it cannot race
    // with itself and a plain increment is enough.
    let entries = ENTRIES.load(Ordering::Relaxed).wrapping_add(1);
    ENTRIES.store(entries, Ordering::Relaxed);
    unsafe {
        // Stop the controller from asserting before anything else. The
        // interrupt is level-triggered all the way up the cascade, so a
        // handler that returns while GINTSTS still matches GINTMSK is re-taken
        // immediately and the foreground never runs again. The foreground
        // re-arms the bits it needs in submit(), reap(), finish(),
        // restart_split() and root_port_state().
        let mask = regs::read(base, regs::GINTMSK);
        regs::write(base, regs::GINTMSK, 0);
        let pending = regs::read(base, regs::GINTSTS) & mask;
        LAST_GINTSTS.store(pending, Ordering::Relaxed);

        if entries.wrapping_sub(OBSERVED.load(Ordering::Relaxed)) > STORM_LIMIT {
            // The foreground has not run for a very long run of interrupts, so
            // this source cannot be serviced here. Take it off the cascade
            // rather than livelock; the driver falls back to polling HCINT.
            STORMED.store(true, Ordering::Relaxed);
            Armctrl::disable(Irq(IRQ.load(Ordering::Relaxed)));
            return;
        }

        if pending & (1 << 24) != 0 {
            let port = regs::read(base, regs::HPRT);
            PORT_EVENTS.store(
                PORT_EVENTS.load(Ordering::Relaxed) | port,
                Ordering::Relaxed,
            );
            // The connect/enable/over-current change bits are W1C and are the
            // only reason the port interrupt asserts, so they have to be
            // written back as 1 to clear.  Bit 2 is the enable bit, which is
            // also W1C and would disable the port, so it is always written 0.
            let changes = port & 0x2a;
            if changes != 0 {
                regs::write(base, regs::HPRT, (port & !0x2e) | changes);
            }
        }
        if pending & (1 << 25) != 0 {
            let haint = regs::read(base, regs::HAINT);
            for channel in 0..16 {
                if haint & (1 << channel) != 0 {
                    let status = regs::read(base, regs::channel(channel, regs::HCINT));
                    // A halted channel cannot make forward progress until the
                    // foreground owner reaps it. Mask it before publishing so
                    // a level-triggered summary cannot starve foreground work.
                    regs::write(base, regs::channel(channel, regs::HCINTMSK), 0);
                    regs::write(
                        base,
                        regs::HAINTMSK,
                        regs::read(base, regs::HAINTMSK) & !(1 << channel),
                    );
                    let published = CHANNEL_EVENTS[channel].load(Ordering::Relaxed) | status;
                    CHANNEL_EVENTS[channel].store(published, Ordering::Release);
                    regs::write(base, regs::channel(channel, regs::HCINT), status)
                }
            }
        }
        regs::write(base, regs::GINTSTS, pending);
    }
    EVENT_GENERATION.store(
        EVENT_GENERATION.load(Ordering::Relaxed).wrapping_add(1),
        Ordering::Release,
    );
}
