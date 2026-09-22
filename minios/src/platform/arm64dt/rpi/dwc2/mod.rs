mod interrupt;
mod regs;

use alloc::boxed::Box;
use core::ptr::NonNull;

use libusb::{DataPid, Direction, TransferProgress, TransferType, UsbError, UsbSpeed};

use crate::io::usb::hcd::*;

pub const MAX_CHANNELS: usize = 4;
pub const DMA_BUFFER_SIZE: usize = 512;
// Split scheduling, following the same DWC_otg core in the sibling tab5
// project (src/usb/hcd.rs, `await_packet`), which reached the same problem
// from the other direction and measured its way out.
//
// The translator needs downstream bus time after it accepts a start-split.
// The reference scheduler advances two microframes before the first
// complete-split; re-enabling the channel straight away can put the CSPLIT in
// the same microframe, and stricter hubs answer with a transaction error.
// Further complete-splits after a NYET advance one microframe each.
const SSPLIT_TO_CSPLIT_MICROFRAMES: u32 = 2;
const CSPLIT_RETRY_MICROFRAMES: u32 = 1;
/// Last resort against a translator that answers NYET forever. Abandoning a
/// split anywhere else than its natural boundary is what wedges a hub, so
/// this is set high enough never to be the ordinary way out.
const SPLIT_HARD_ROUND_CAP: u8 = 200;
/// HCINT bits that conclude a packet: transfer complete, AHB error, STALL,
/// transaction error, babble, frame overrun and data toggle error.
///
/// Only a bare handshake keeps a split sequence going. Checking a handshake
/// bit before these is how the ACK that ends an OUT transaction
/// (`XFERCOMPL | ACK | CHHLTD`) gets mistaken for "the translator is still
/// working", after which the host asks for a result that has already been
/// delivered and the channel never halts again.
const HCINT_CONCLUDES: u32 = 1 | (1 << 2) | (1 << 3) | (1 << 7) | (1 << 8) | (1 << 9) | (1 << 10);
/// Last microframe a start-split may be issued in, leaving room for the
/// complete-splits that follow it.
///
/// A periodic sequence is abandoned at the end of the frame its start-split
/// belonged to, so how many complete-split slots remain in that frame decides
/// whether it collects anything. Measured on a Raspberry Pi 3 (2026-09-21),
/// the share of keyboard polls that lost their report tracked exactly that:
/// 7-19% when the start left four slots or more, 66% with three, and 78-82%
/// with two or one. Periodic start-splits are therefore held to microframes
/// 0..=2. Non-periodic transfers are not bound to the frame, because the
/// translator keeps their result until it is collected.
const LAST_PERIODIC_SPLIT_START: u32 = 2;
const LAST_SPLIT_START: u32 = 5;

#[derive(Clone, Copy, Debug)]
pub struct DmaMap {
    pub cpu_start: u64,
    pub bus_start: u64,
    pub length: u64,
}
impl DmaMap {
    pub fn translate(&self, cpu: usize, len: usize) -> Option<u32> {
        let cpu = cpu as u64;
        let end = cpu.checked_add(len as u64)?;
        if cpu < self.cpu_start || end > self.cpu_start.checked_add(self.length)? {
            return None;
        }
        u32::try_from(self.bus_start.checked_add(cpu - self.cpu_start)?).ok()
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Capabilities {
    pub snpsid: u32,
    pub hwcfg: [u32; 4],
    /// GHWCFG4 bit 30: the core can walk per-(micro)frame descriptor lists
    /// and schedule periodic split transactions itself through
    /// `HCTSIZ.SCHED_INFO`. This driver uses buffer DMA and drives both by
    /// hand, so it only reports the capability.
    pub descriptor_dma: bool,
    pub host_channels: u8,
    pub fifo_depth_words: u16,
    pub dynamic_fifo: bool,
    pub dma_architecture: u8,
    pub hs_phy_type: u8,
    pub utmi_width: u8,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct RegisterSnapshot {
    pub gusbcfg: u32,
    pub hcfg: u32,
    pub hprt: u32,
    pub pcgctl: u32,
    pub gintsts: u32,
}

#[repr(C, align(64))]
#[derive(Clone, Copy)]
struct DmaBuffer([u8; DMA_BUFFER_SIZE]);
#[derive(Clone, Copy)]
struct Slot {
    generation: u32,
    active: bool,
    retiring: bool,
    direction: Direction,
    transfer_type: TransferType,
    caller: Option<NonNull<u8>>,
    length: usize,
    programmed_length: usize,
    dma: u32,
    hctsiz: u32,
    deadline: u64,
    split: bool,
    complete_split: bool,
    split_retries: u8,
    split_pending: bool,
    /// HFNUM value the next split phase may be issued at.
    split_target_frame: u32,
    /// The high-speed frame the start-split belonged to. A periodic split's
    /// whole sequence lives in one frame; once it advances, the translator
    /// has let the transaction expire and there is nothing left to collect.
    split_start_frame: u32,
    /// Microframe this split transfer was started in.
    start_microframe: u8,
}
impl Slot {
    const EMPTY: Self = Self {
        generation: 0,
        active: false,
        retiring: false,
        direction: Direction::Out,
        transfer_type: TransferType::Control,
        caller: None,
        length: 0,
        programmed_length: 0,
        dma: 0,
        hctsiz: 0,
        deadline: 0,
        split: false,
        complete_split: false,
        split_retries: 0,
        split_pending: false,
        split_target_frame: 0,
        split_start_frame: 0,
        start_microframe: 0,
    };
}

pub struct Dwc2 {
    base: usize,
    dma: DmaMap,
    capabilities: Capabilities,
    buffers: Box<[DmaBuffer; MAX_CHANNELS]>,
    slots: [Slot; MAX_CHANNELS],
    timed_out: [Option<TransferCompletion>; MAX_CHANNELS],
    snapshot: HcdSnapshot,
    reset_deadline: Option<u64>,
    storm_reported: bool,
}

impl Dwc2 {
    pub unsafe fn new(base: usize, dma: DmaMap, now_us: fn() -> u64) -> Result<Self, UsbError> {
        let snpsid = unsafe { regs::read(base, regs::GSNPSID) };
        if snpsid & 0xffff_0000 != 0x4f54_0000 {
            return Err(UsbError::Unsupported);
        }
        let hwcfg = [
            unsafe { regs::read(base, regs::GHWCFG1) },
            unsafe { regs::read(base, regs::GHWCFG2) },
            unsafe { regs::read(base, regs::GHWCFG3) },
            unsafe { regs::read(base, regs::GHWCFG4) },
        ];
        let host_channels = (((hwcfg[1] >> 14) & 0xf) + 1) as u8;
        let initial_gusbcfg = unsafe { regs::read(base, regs::GUSBCFG) };
        let utmi_width = match (hwcfg[3] >> 14) & 3 {
            1 => 16,
            2 if initial_gusbcfg & (1 << 3) != 0 => 16,
            _ => 8,
        };
        let cap = Capabilities {
            snpsid,
            hwcfg,
            host_channels,
            fifo_depth_words: (hwcfg[2] >> 16) as u16,
            dynamic_fifo: hwcfg[1] & (1 << 19) != 0,
            dma_architecture: ((hwcfg[1] >> 3) & 3) as u8,
            hs_phy_type: ((hwcfg[1] >> 6) & 3) as u8,
            utmi_width,
            descriptor_dma: hwcfg[3] & (1 << 30) != 0,
        };
        if cap.host_channels == 0 || cap.dma_architecture == 0 {
            return Err(UsbError::Unsupported);
        }
        let mut this = Self {
            base,
            dma,
            capabilities: cap,
            buffers: Box::new([DmaBuffer([0; DMA_BUFFER_SIZE]); MAX_CHANNELS]),
            slots: [Slot::EMPTY; MAX_CHANNELS],
            timed_out: [None; MAX_CHANNELS],
            snapshot: HcdSnapshot::default(),
            reset_deadline: None,
            storm_reported: false,
        };
        unsafe { this.initialize(now_us)? };
        Ok(this)
    }
    unsafe fn wait_register(
        &self,
        offset: usize,
        mask: u32,
        set: bool,
        timeout_us: u64,
        now_us: fn() -> u64,
    ) -> Result<(), UsbError> {
        let start = now_us();
        loop {
            let value = unsafe { regs::read(self.base, offset) };
            if (value & mask != 0) == set {
                return Ok(());
            }
            if now_us().wrapping_sub(start) >= timeout_us {
                return Err(UsbError::ControllerFault);
            }
            core::hint::spin_loop();
        }
    }

    unsafe fn initialize(&mut self, now_us: fn() -> u64) -> Result<(), UsbError> {
        unsafe {
            regs::write(self.base, regs::GINTMSK, 0);
            regs::write(self.base, regs::GAHBCFG, 0);

            // Firmware may leave the core or PHY clock gated.  PHY selection
            // bits survive a core reset and must be programmed before it.
            regs::write(self.base, regs::PCGCTL, 0);
            let mut usbcfg = regs::read(self.base, regs::GUSBCFG);
            usbcfg &= !(1 << 30);
            usbcfg |= 1 << 29;
            match self.capabilities.hs_phy_type {
                // UTMI+ or UTMI+/ULPI: Raspberry Pi uses the UTMI+ path.
                1 | 3 => {
                    usbcfg &= !((1 << 6) | (1 << 4) | (1 << 3));
                    if self.capabilities.utmi_width == 16 {
                        usbcfg |= 1 << 3;
                    }
                }
                // ULPI.
                2 => {
                    usbcfg &= !(1 << 6);
                    usbcfg |= 1 << 4;
                    usbcfg &= !(1 << 3);
                }
                // A core with only a dedicated FS PHY keeps its firmware PHY
                // selection.  This is not expected on a Raspberry Pi 3.
                _ => {}
            }
            usbcfg &= !((1 << 19) | (1 << 17) | 7);
            usbcfg |= 7;
            regs::write(self.base, regs::GUSBCFG, usbcfg);

            self.wait_register(regs::GRSTCTL, 1 << 31, true, 10_000, now_us)?;
            regs::write(self.base, regs::GRSTCTL, 1);
            self.wait_register(regs::GRSTCTL, 1, false, 10_000, now_us)?;
            self.wait_register(regs::GRSTCTL, 1 << 31, true, 10_000, now_us)?;
            self.wait_register(regs::GINTSTS, 1, true, 120_000, now_us)?;

            // Override the external VBUS-valid input, which is not wired on
            // every integrated DWC2 implementation, and restart PHY clocks.
            regs::write(
                self.base,
                regs::GOTGCTL,
                regs::read(self.base, regs::GOTGCTL) | (1 << 2) | (1 << 3),
            );
            regs::write(self.base, regs::PCGCTL, 0);

            if self.capabilities.dynamic_fifo {
                let depth = self.capabilities.fifo_depth_words.max(256) as u32;
                // BCM2835 needs a larger host RX FIFO than the generic DWC2
                // default.  Preserve the reset TX depths when they fit, as
                // those describe the integration's intended FIFO layout.
                let rx = 774.min(depth.saturating_sub(32)).max(16);
                let available = depth - rx;
                let reset_np = regs::read(self.base, regs::GNPTXFSIZ) >> 16;
                let reset_periodic = regs::read(self.base, regs::HPTXFSIZ) >> 16;
                let (np, periodic) = if reset_np >= 16
                    && reset_periodic >= 16
                    && reset_np + reset_periodic <= available
                {
                    (reset_np, reset_periodic)
                } else {
                    (available / 2, available - available / 2)
                };
                regs::write(self.base, regs::GRXFSIZ, rx);
                regs::write(self.base, regs::GNPTXFSIZ, (np << 16) | rx);
                regs::write(self.base, regs::HPTXFSIZ, (periodic << 16) | (rx + np))
            }

            // A HS PHY requires the 30/60 MHz FS/LS clock selection.  The
            // previous unconditional value 1 selected the dedicated-FS 48 MHz
            // clock and leaves the Pi PHY unable to observe its root hub.
            let hcfg = regs::read(self.base, regs::HCFG) & !3;
            regs::write(
                self.base,
                regs::HCFG,
                hcfg | u32::from(self.capabilities.hs_phy_type == 0),
            );

            // Discard any request state inherited from firmware/bootloader.
            regs::write(self.base, regs::GRSTCTL, (0x10 << 6) | (1 << 5));
            self.wait_register(regs::GRSTCTL, 1 << 5, false, 10_000, now_us)?;
            regs::write(self.base, regs::GRSTCTL, 1 << 4);
            self.wait_register(regs::GRSTCTL, 1 << 4, false, 10_000, now_us)?;
            for channel in 0..usize::from(self.capabilities.host_channels) {
                regs::write(self.base, regs::channel(channel, regs::HCINTMSK), 0);
                regs::write(self.base, regs::channel(channel, regs::HCINT), u32::MAX);
                let hcchar = regs::read(self.base, regs::channel(channel, regs::HCCHAR));
                if hcchar & (1 << 31) != 0 {
                    regs::write(
                        self.base,
                        regs::channel(channel, regs::HCCHAR),
                        (hcchar & !((1 << 31) | (1 << 15))) | (1 << 30),
                    );
                }
            }
            for channel in 0..usize::from(self.capabilities.host_channels) {
                let hcchar = regs::read(self.base, regs::channel(channel, regs::HCCHAR));
                if hcchar & (1 << 31) != 0 {
                    regs::write(
                        self.base,
                        regs::channel(channel, regs::HCCHAR),
                        (hcchar & !(1 << 15)) | (1 << 31) | (1 << 30),
                    );
                    self.wait_register(
                        regs::channel(channel, regs::HCCHAR),
                        1 << 31,
                        false,
                        1_000,
                        now_us,
                    )?;
                }
            }

            let port = regs::read(self.base, regs::HPRT);
            regs::write(self.base, regs::HPRT, (port & !0x2e) | (1 << 12));
            regs::write(self.base, regs::GINTSTS, u32::MAX);
            regs::write(
                self.base,
                regs::HAINTMSK,
                (1u32 << self.capabilities.host_channels.min(MAX_CHANNELS as u8)) - 1,
            );
            regs::write(self.base, regs::GINTMSK, (1 << 24) | (1 << 25));
            // Bit 4 is the BCM2835 wait-for-AXI-writes integration setting.
            // Bit 5 enables internal DMA and bit 0 enables global interrupts.
            regs::write(self.base, regs::GAHBCFG, (1 << 5) | (1 << 4) | 1);
            Ok(())
        }
    }
    pub const fn capabilities(&self) -> Capabilities {
        self.capabilities
    }
    pub fn register_snapshot(&self) -> RegisterSnapshot {
        unsafe {
            RegisterSnapshot {
                gusbcfg: regs::read(self.base, regs::GUSBCFG),
                hcfg: regs::read(self.base, regs::HCFG),
                hprt: regs::read(self.base, regs::HPRT),
                pcgctl: regs::read(self.base, regs::PCGCTL),
                gintsts: regs::read(self.base, regs::GINTSTS),
            }
        }
    }
    fn speed(&self) -> UsbSpeed {
        let hprt = unsafe { regs::read(self.base, regs::HPRT) };
        match (hprt >> 17) & 3 {
            0 => UsbSpeed::High,
            2 => UsbSpeed::Low,
            _ => UsbSpeed::Full,
        }
    }
    fn finish(&mut self, channel: usize, status: u32) -> TransferCompletion {
        let slot = &mut self.slots[channel];
        let token = TransferToken::new(channel as u8, slot.generation);
        let hctsiz = unsafe { regs::read(self.base, regs::channel(channel, regs::HCTSIZ)) };
        let hcsplt = unsafe { regs::read(self.base, regs::channel(channel, regs::HCSPLT)) };
        let remaining = (hctsiz & 0x7ffff) as usize;
        // IN transfers are programmed for whole max-packet buffers.  The
        // device can terminate them with a short packet, so derive the USB
        // byte count from that programmed size and then cap it to the
        // caller's logical request length.
        //
        // An OUT transfer is different: the core does not count HCTSIZ down
        // as it sends, so "programmed - remaining" reads as zero on a real
        // Raspberry Pi 3 even though the device acknowledged everything
        // (HCINT = XFERCOMPL | CHHLTD | ACK).  QEMU does count it down, which
        // hid this until the first bulk OUT (a mass storage CBW) ran on
        // hardware.  Linux's dwc2 likewise takes a completed non-split OUT
        // as its whole length (`dwc2_get_actual_xfer_length`).  Only
        // XFERCOMPL says that; any other halt reports no progress below.
        let actual = if slot.direction == Direction::Out && status & 1 != 0 {
            slot.length
        } else {
            slot.programmed_length
                .saturating_sub(remaining)
                .min(slot.length)
        };
        if status & (1 << 10) != 0 {
            unsafe {
                crate::usb_println!(
                    "USB DWC2 toggle HC{}: TSIZ={:08x} saved={:08x} SPLT={:08x} DMA={:08x}/{:08x} HFNUM={:08x} C={} R={}",
                    channel,
                    hctsiz,
                    slot.hctsiz,
                    hcsplt,
                    regs::read(self.base, regs::channel(channel, regs::HCDMA)),
                    slot.dma,
                    regs::read(self.base, regs::HFNUM),
                    slot.complete_split as u8,
                    slot.split_retries,
                );
            }
        }
        let result = if status & 1 != 0 {
            if slot.direction == Direction::In && actual > 0 {
                unsafe {
                    crate::arch::cache::dcache_invalidate(
                        self.buffers[channel].0.as_ptr() as usize,
                        actual,
                    );
                    core::ptr::copy_nonoverlapping(
                        self.buffers[channel].0.as_ptr(),
                        slot.caller.unwrap().as_ptr(),
                        actual,
                    )
                }
            }
            Ok(actual)
        } else if status & ((1 << 9) | (1 << 10) | (1 << 14)) != 0 {
            // Data-toggle errors can be reported together with STALL on the
            // Pi 3 DWC2. Prefer the recoverable transaction classification so
            // a control request is restarted from SETUP and resynchronizes
            // endpoint zero.
            Err(UsbError::Transaction)
        } else if status & (1 << 3) != 0 {
            Err(UsbError::Stall)
        } else if status & (1 << 4) != 0 {
            Err(UsbError::Nak)
        } else if status & (1 << 6) != 0 {
            Err(UsbError::Nyet)
        } else if status & (1 << 7) != 0 {
            Err(UsbError::Transaction)
        } else if status & (1 << 8) != 0 {
            Err(UsbError::Babble)
        } else {
            crate::usb_println!(
                "USB DWC2 HC{} unexpected: INT={:08x} TSIZ={:08x} SPLT={:08x} C={} R={}",
                channel,
                status,
                hctsiz,
                hcsplt,
                slot.complete_split as u8,
                slot.split_retries
            );
            Err(UsbError::ControllerFault)
        };
        slot.active = false;
        slot.split_pending = false;
        self.snapshot.active = self.snapshot.active.saturating_sub(1);
        self.snapshot.reaped = self.snapshot.reaped.saturating_add(1);
        let counter = match result {
            Ok(_) => &mut self.snapshot.completed,
            Err(UsbError::Nak) => &mut self.snapshot.naks,
            Err(UsbError::Nyet) => &mut self.snapshot.nyets,
            Err(UsbError::Stall) => &mut self.snapshot.stalls,
            Err(UsbError::Timeout) => &mut self.snapshot.timeouts,
            Err(UsbError::Transaction) => &mut self.snapshot.transaction_errors,
            Err(_) => &mut self.snapshot.other_errors,
        };
        *counter = counter.saturating_add(1);
        unsafe {
            regs::write(
                self.base,
                regs::GINTMSK,
                regs::read(self.base, regs::GINTMSK) | (1 << 25),
            );
        }
        TransferCompletion {
            token,
            result,
            progress: if status & 1 != 0 {
                TransferProgress::Known(actual)
            } else if status & (1 << 4) != 0 {
                // A NAK consumes no payload and is safe to retry.
                TransferProgress::Known(0)
            } else {
                TransferProgress::Unknown
            },
        }
    }

    const fn last_split_start(transfer_type: TransferType) -> u32 {
        match transfer_type {
            TransferType::Interrupt | TransferType::Isochronous => LAST_PERIODIC_SPLIT_START,
            _ => LAST_SPLIT_START,
        }
    }

    /// The host frame counter, which counts microframes at high speed.
    fn frame(&self) -> u32 {
        let value = unsafe { regs::read(self.base, regs::HFNUM) };
        value & 0x3fff
    }

    /// True once `now` has reached `target`, across the counter's wrap.
    const fn frame_reached(now: u32, target: u32) -> bool {
        now.wrapping_sub(target) & 0x3fff < 0x2000
    }

    /// Schedules the next phase of a split a number of microframes out.
    ///
    /// The wait is measured against the core's own frame counter rather than
    /// a microsecond clock: the translator's windows are microframes, and a
    /// software timer drifts against them.
    fn schedule_split(&mut self, channel: usize, complete: bool, microframes: u32) {
        let target = self.frame().wrapping_add(microframes) & 0x3fff;
        let slot = &mut self.slots[channel];
        slot.complete_split = complete;
        slot.split_retries = slot.split_retries.saturating_add(1);
        slot.split_pending = true;
        slot.split_target_frame = target;
    }

    fn restart_split(&mut self, channel: usize) {
        let frame = self.frame();
        let slot = &mut self.slots[channel];
        let complete = slot.complete_split;
        if !complete {
            let microframe = frame & 7;
            if microframe > Self::last_split_start(slot.transfer_type) {
                // A start-split has to leave room for the complete-splits
                // that follow it inside the same frame.
                self.snapshot.split_deferrals = self.snapshot.split_deferrals.saturating_add(1);
                slot.split_pending = true;
                slot.split_target_frame = frame.wrapping_add(8 - microframe) & 0x3fff;
                return;
            }
            // This start-split opens a new window; the complete-splits that
            // follow belong to this frame.
            slot.split_start_frame = frame >> 3;
        }
        slot.split_pending = false;
        unsafe {
            let split = regs::read(self.base, regs::channel(channel, regs::HCSPLT));
            let split = if complete {
                split | (1 << 16)
            } else {
                split & !(1 << 16)
            };
            regs::write(self.base, regs::channel(channel, regs::HCSPLT), split);
            let hctsiz = if complete && slot.direction == Direction::Out {
                // A complete-split OUT carries only the handshake; the data
                // was sent in the start-split transaction.
                (slot.hctsiz & !0x1fff_ffff) | (1 << 19)
            } else {
                slot.hctsiz
            };
            // The core updates HCTSIZ.PID and the DMA pointer while a
            // channel runs, including attempts that end in NAK/NYET.  A
            // split retry is the same USB packet, so rebuild both registers
            // from the saved request instead of reusing those live values.
            regs::write(self.base, regs::channel(channel, regs::HCDMA), slot.dma);
            regs::write(self.base, regs::channel(channel, regs::HCTSIZ), hctsiz);
            regs::write(self.base, regs::channel(channel, regs::HCINT), u32::MAX);
            // In buffer-DMA mode the controller halts the channel before the
            // software examines the latched completion reason.  ACK/NAK may
            // arrive earlier and must not be mistaken for final completion.
            regs::write(self.base, regs::channel(channel, regs::HCINTMSK), 0x6);
            regs::write(
                self.base,
                regs::HAINTMSK,
                regs::read(self.base, regs::HAINTMSK) | (1 << channel),
            );
            regs::write(
                self.base,
                regs::GINTMSK,
                regs::read(self.base, regs::GINTMSK) | (1 << 25),
            );
            let mut channel_character =
                regs::read(self.base, regs::channel(channel, regs::HCCHAR)) & !(1 << 30);
            if matches!(
                slot.transfer_type,
                TransferType::Interrupt | TransferType::Isochronous
            ) {
                // service_timeouts() calls us after the required split-phase
                // delay. Select the current (micro)frame so CSPLIT is issued
                // now; selecting the next one adds 125 us and can miss the
                // transaction translator's completion window.
                channel_character &= !(1 << 29);
                if regs::read(self.base, regs::HFNUM) & 1 != 0 {
                    channel_character |= 1 << 29;
                }
            }
            regs::write(
                self.base,
                regs::channel(channel, regs::HCCHAR),
                channel_character | (1 << 31),
            );
        }
    }
}

impl HostController for Dwc2 {
    fn root_port_state(&self) -> RootPortState {
        unsafe {
            regs::write(
                self.base,
                regs::GINTMSK,
                regs::read(self.base, regs::GINTMSK) | (1 << 24),
            );
        }
        let p = unsafe { regs::read(self.base, regs::HPRT) };
        if p & 1 == 0 {
            RootPortState::Disconnected
        } else if p & (1 << 2) != 0 {
            RootPortState::Enabled(self.speed())
        } else {
            RootPortState::Connected(self.speed())
        }
    }
    fn reset_root_port(&mut self, deadline_us: u64) -> Result<(), UsbError> {
        let p = unsafe { regs::read(self.base, regs::HPRT) };
        unsafe { regs::write(self.base, regs::HPRT, (p & !0x2e) | (1 << 8)) };
        self.reset_deadline = Some(deadline_us);
        Ok(())
    }
    fn submit(&mut self, request: TransferRequest<'_>) -> Result<TransferToken, UsbError> {
        if request.buffer.len() > DMA_BUFFER_SIZE {
            return Err(UsbError::ResourceExhausted);
        }
        if request.max_packet_size == 0 {
            return Err(UsbError::InvalidRequest);
        }
        let channel = self
            .slots
            .iter()
            .position(|s| !s.active && !s.retiring)
            .ok_or(UsbError::ResourceExhausted)?;
        let dma = self
            .dma
            .translate(
                self.buffers[channel].0.as_ptr() as usize,
                request.buffer.len(),
            )
            .ok_or(UsbError::Dma)?;
        let slot = &mut self.slots[channel];
        slot.generation = slot.generation.wrapping_add(1).max(1);
        slot.active = true;
        slot.retiring = false;
        slot.direction = request.direction;
        slot.transfer_type = request.transfer_type;
        slot.caller = NonNull::new(request.buffer.as_mut_ptr());
        slot.length = request.buffer.len();
        slot.programmed_length = slot.length;
        slot.dma = dma;
        slot.deadline = request.deadline_us;
        slot.split = request.route.translator.is_some();
        slot.complete_split = false;
        slot.split_retries = 0;
        slot.split_pending = false;
        slot.split_target_frame = 0;
        slot.split_start_frame = 0;
        slot.start_microframe = 0;
        if request.direction == Direction::Out {
            self.buffers[channel].0[..slot.length].copy_from_slice(request.buffer);
            unsafe {
                crate::arch::cache::dcache_clean(
                    self.buffers[channel].0.as_ptr() as usize,
                    slot.length,
                )
            }
        } else if slot.length != 0 {
            // Evict dirty allocator/previous-transfer data before ownership is
            // handed to a DMA writer.  Invalidating only after completion can
            // otherwise write stale cache data over the received packet.
            unsafe {
                crate::arch::cache::dcache_clean_invalidate(
                    self.buffers[channel].0.as_ptr() as usize,
                    slot.length,
                )
            }
        }
        let max_packet_size = request.max_packet_size as usize;
        let packets = (slot.length.max(1) + max_packet_size - 1) / max_packet_size;
        // DWC2 expects HCTSIZ.XferSize for an IN channel to describe an
        // integral number of maximum-size packets.  This is especially
        // important for split INs: programming the final short length (for
        // example one byte of a 9-byte descriptor) can leave the TT/control
        // endpoint toggle out of sync for the following request.
        if request.direction == Direction::In {
            slot.programmed_length = packets * max_packet_size;
        }
        let pid = match request.pid {
            DataPid::Data0 => 0,
            DataPid::Data1 => 2,
            DataPid::Setup => 3,
        };
        let hctsiz = (slot.programmed_length as u32) | ((packets as u32) << 19) | (pid << 29);
        slot.hctsiz = hctsiz;
        unsafe {
            regs::write(self.base, regs::channel(channel, regs::HCINT), u32::MAX);
            regs::write(self.base, regs::channel(channel, regs::HCINTMSK), 0x6);
            regs::write(
                self.base,
                regs::HAINTMSK,
                regs::read(self.base, regs::HAINTMSK) | (1 << channel),
            );
            regs::write(
                self.base,
                regs::GINTMSK,
                regs::read(self.base, regs::GINTMSK) | (1 << 25),
            );
            regs::write(self.base, regs::channel(channel, regs::HCDMA), dma);
            regs::write(self.base, regs::channel(channel, regs::HCTSIZ), hctsiz);
            let split = request
                .route
                .translator
                .map(|t| {
                    // Non-isochronous split transactions always describe the
                    // complete payload.  XACTPOS=ALL is required by DWC2 for
                    // control/bulk/interrupt transfers through a high-speed
                    // hub; leaving it at MID is tolerated by QEMU's
                    // full-speed topology but rejected by real hardware.
                    t.port_number as u32
                        | ((t.hub_address.get() as u32) << 7)
                        | (3 << 14)
                        | (1 << 31)
                })
                .unwrap_or(0);
            regs::write(self.base, regs::channel(channel, regs::HCSPLT), split);
            let kind = match request.transfer_type {
                libusb::TransferType::Control => 0,
                libusb::TransferType::Isochronous => 1,
                libusb::TransferType::Bulk => 2,
                libusb::TransferType::Interrupt => 3,
            };
            let mut ch = (request.max_packet_size as u32)
                | ((request.endpoint.number() as u32) << 11)
                | (((request.direction == Direction::In) as u32) << 15)
                | (((request.route.device_speed == UsbSpeed::Low) as u32) << 17)
                | (kind << 18)
                | (1 << 20)
                | ((request.address.get() as u32) << 22);
            if matches!(
                request.transfer_type,
                TransferType::Interrupt | TransferType::Isochronous
            ) {
                // Start periodic transfers in the next (micro)frame. This
                // avoids enabling a channel too late for the current frame.
                // A split transfer never reaches this on its own; its start
                // is issued from restart_split(), which sets this field
                // itself.
                let frame = regs::read(self.base, regs::HFNUM);
                if frame & 1 == 0 {
                    ch |= 1 << 29;
                }
            }
            let defer_split_start = if slot.split {
                let microframe_now = regs::read(self.base, regs::HFNUM) & 0x3fff;
                let microframe = microframe_now & 7;
                if matches!(
                    request.transfer_type,
                    TransferType::Interrupt | TransferType::Isochronous
                ) {
                    slot.start_microframe = microframe as u8;
                    self.snapshot.periodic_starts[microframe as usize] =
                        self.snapshot.periodic_starts[microframe as usize].saturating_add(1);
                }
                slot.split_start_frame = microframe_now >> 3;
                if microframe > Self::last_split_start(request.transfer_type) {
                    self.snapshot.split_deferrals = self.snapshot.split_deferrals.saturating_add(1);
                    slot.split_pending = true;
                    slot.split_target_frame = microframe_now.wrapping_add(8 - microframe) & 0x3fff;
                    true
                } else {
                    false
                }
            } else {
                false
            };
            if !defer_split_start {
                ch |= 1 << 31;
            }
            regs::write(self.base, regs::channel(channel, regs::HCCHAR), ch)
        }
        self.snapshot.submitted = self.snapshot.submitted.saturating_add(1);
        self.snapshot.active = self.snapshot.active.saturating_add(1);
        Ok(TransferToken::new(channel as u8, slot.generation))
    }
    fn cancel(&mut self, token: TransferToken) -> Result<TransferProgress, UsbError> {
        let channel = token.slot() as usize;
        let slot = self
            .slots
            .get_mut(channel)
            .ok_or(UsbError::InvalidRequest)?;
        if !slot.active || slot.generation != token.generation() {
            self.snapshot.stale_tokens = self.snapshot.stale_tokens.saturating_add(1);
            return Err(UsbError::InvalidRequest);
        }
        unsafe {
            let ch = regs::read(self.base, regs::channel(channel, regs::HCCHAR));
            regs::write(
                self.base,
                regs::channel(channel, regs::HCCHAR),
                ch | (1 << 30) | (1 << 31),
            )
        }
        slot.active = false;
        slot.split_pending = false;
        slot.retiring = true;
        self.snapshot.active = self.snapshot.active.saturating_sub(1);
        self.snapshot.cancelled = self.snapshot.cancelled.saturating_add(1);
        Ok(TransferProgress::Unknown)
    }
    fn reap(&mut self) -> Option<TransferCompletion> {
        interrupt::note_progress();
        if interrupt::stormed() && !self.storm_reported {
            self.storm_reported = true;
            crate::usb_println!("USB DWC2: interrupt unserviceable, falling back to polling");
        }
        for completion in &mut self.timed_out {
            if let Some(completion) = completion.take() {
                self.snapshot.reaped = self.snapshot.reaped.saturating_add(1);
                return Some(completion);
            }
        }
        if self.slots.iter().any(|s| s.active) {
            // The ISR masks the host-channel summary before publishing, and
            // only finish() puts it back.  A poll that retires a cancelled
            // generation would otherwise leave the remaining channels without
            // an interrupt until their deadline.
            unsafe {
                regs::write(
                    self.base,
                    regs::GINTMSK,
                    regs::read(self.base, regs::GINTMSK) | (1 << 25),
                );
            }
        }
        for channel in 0..MAX_CHANNELS {
            // When the ISR is installed it latches and acknowledges HCINT
            // before the foreground can see it, so take what it published.
            let published = interrupt::take_channel_events(channel);
            // Raspberry Pi's GPU-interrupt cascade is not required for
            // correctness: foreground polling can reap a halted DMA channel
            // directly.  This also avoids losing USB completely if the legacy
            // ARMCTRL route is unavailable or remains asserted.
            let hardware = unsafe {
                let status = regs::read(self.base, regs::channel(channel, regs::HCINT));
                if status & 0x6 != 0 {
                    regs::write(self.base, regs::channel(channel, regs::HCINTMSK), 0);
                    regs::write(
                        self.base,
                        regs::HAINTMSK,
                        regs::read(self.base, regs::HAINTMSK) & !(1 << channel),
                    );
                    regs::write(self.base, regs::channel(channel, regs::HCINT), status);
                    status
                } else {
                    0
                }
            };
            let status = published | hardware;
            if status != 0 {
                self.snapshot.last_interrupt = status;
                if self.slots[channel].retiring {
                    // This is the terminal halt for a cancelled generation.
                    // Only after consuming it may the channel be reused.
                    self.slots[channel].retiring = false;
                    continue;
                }
                if self.slots[channel].active {
                    let slot = self.slots[channel];
                    // Anything that concludes the packet belongs to finish(),
                    // whatever handshake bit accompanies it.
                    let concludes = status & HCINT_CONCLUDES != 0;
                    if slot.split && !concludes && status & (1 << 1) != 0 && status & (1 << 5) != 0
                    {
                        // An ACK is a handshake, never a conclusion: on a
                        // start-split it means the translator took the
                        // transaction, and on a complete-split it means it is
                        // still working on it. Either way the answer is to
                        // ask for the result, two microframes after a start
                        // and one after a previous complete.
                        let microframes = if slot.complete_split {
                            CSPLIT_RETRY_MICROFRAMES
                        } else {
                            SSPLIT_TO_CSPLIT_MICROFRAMES
                        };
                        self.schedule_split(channel, true, microframes);
                        continue;
                    }
                    if slot.split
                        && !concludes
                        && slot.transfer_type == TransferType::Interrupt
                        && status & (1 << 1) != 0
                        && status & (1 << 4) != 0
                    {
                        self.snapshot.split_naks = self.snapshot.split_naks.saturating_add(1);
                        // An interrupt IN NAK means there is no report for
                        // this service interval. Complete this poll so the HID
                        // layer waits for bInterval instead of exhausting the
                        // split retry budget on an idle keyboard.
                        return Some(self.finish(channel, status));
                    }
                    // Walking away mid-sequence leaves the translator holding
                    // a transaction nobody collects, and the next unrelated
                    // transfer to that hub then fails: a complete-split may
                    // only be abandoned once the translator has let go. For a
                    // periodic transfer that moment is the end of the frame
                    // its start-split belonged to, not a retry count, so the
                    // count below is only a last resort against a translator
                    // that answers NYET forever.
                    let expired = slot.split
                        && matches!(
                            slot.transfer_type,
                            TransferType::Interrupt | TransferType::Isochronous
                        )
                        && slot.complete_split
                        && (self.frame() >> 3 != slot.split_start_frame || self.frame() & 7 == 7);
                    let split_retry_limit = if slot.transfer_type == TransferType::Interrupt {
                        SPLIT_HARD_ROUND_CAP
                    } else {
                        64
                    };
                    if slot.split
                        && !concludes
                        && status & (1 << 1) != 0
                        && slot.split_retries < split_retry_limit
                        && !expired
                    {
                        if slot.complete_split && status & (1 << 6) != 0 {
                            self.snapshot.split_csplit_retries =
                                self.snapshot.split_csplit_retries.saturating_add(1);
                            self.schedule_split(channel, true, CSPLIT_RETRY_MICROFRAMES);
                            continue;
                        }
                        if status & (1 << 4) != 0 {
                            // A NAK releases the translator's buffer, so the
                            // sequence starts over from a fresh start-split.
                            self.schedule_split(channel, false, CSPLIT_RETRY_MICROFRAMES);
                            continue;
                        }
                    }
                    if expired && !concludes {
                        self.snapshot.split_expired = self.snapshot.split_expired.saturating_add(1);
                    }
                    if slot.split && !concludes && slot.split_retries >= split_retry_limit {
                        self.snapshot.split_exhausted =
                            self.snapshot.split_exhausted.saturating_add(1);
                    }
                    if !concludes
                        && (expired || slot.split_retries >= split_retry_limit)
                        && slot.split
                    {
                        if matches!(
                            slot.transfer_type,
                            TransferType::Interrupt | TransferType::Isochronous
                        ) {
                            let uframe = (slot.start_microframe & 7) as usize;
                            self.snapshot.periodic_losses[uframe] =
                                self.snapshot.periodic_losses[uframe].saturating_add(1);
                        }
                    }
                    return Some(self.finish(channel, status));
                }
                self.snapshot.stale_tokens = self.snapshot.stale_tokens.saturating_add(1)
            }
        }
        None
    }
    fn service_timeouts(&mut self, now_us: u64) {
        if let Some(deadline) = self.reset_deadline
            && now_us >= deadline
        {
            let p = unsafe { regs::read(self.base, regs::HPRT) };
            // HPRT change bits are W1C and writing the read-only enable bit
            // back as 1 can disable the port on some DWC2 revisions.
            unsafe { regs::write(self.base, regs::HPRT, p & !0x12e) };
            self.reset_deadline = None
        }
        for channel in 0..MAX_CHANNELS {
            if self.slots[channel].active
                && self.slots[channel].split_pending
                && Self::frame_reached(self.frame(), self.slots[channel].split_target_frame)
                && now_us < self.slots[channel].deadline
            {
                self.restart_split(channel);
            }
            if self.slots[channel].active && now_us >= self.slots[channel].deadline {
                if self.slots[channel].transfer_type == TransferType::Interrupt {
                    unsafe {
                        crate::usb_println!(
                            "USB DWC2 HID timeout HC{}: INT={:08x} CHAR={:08x} TSIZ={:08x} SPLT={:08x} C={} R={}",
                            channel,
                            regs::read(self.base, regs::channel(channel, regs::HCINT)),
                            regs::read(self.base, regs::channel(channel, regs::HCCHAR)),
                            regs::read(self.base, regs::channel(channel, regs::HCTSIZ)),
                            regs::read(self.base, regs::channel(channel, regs::HCSPLT)),
                            self.slots[channel].complete_split as u8,
                            self.slots[channel].split_retries,
                        );
                    }
                }
                let slot = &mut self.slots[channel];
                let token = TransferToken::new(channel as u8, slot.generation);
                unsafe {
                    self.snapshot.last_interrupt =
                        regs::read(self.base, regs::channel(channel, regs::HCINT));
                    let ch = regs::read(self.base, regs::channel(channel, regs::HCCHAR));
                    regs::write(
                        self.base,
                        regs::channel(channel, regs::HCCHAR),
                        ch | (1 << 30) | (1 << 31),
                    );
                }
                slot.active = false;
                slot.split_pending = false;
                slot.retiring = true;
                self.snapshot.active = self.snapshot.active.saturating_sub(1);
                self.timed_out[channel] = Some(TransferCompletion {
                    token,
                    result: Err(UsbError::Timeout),
                    progress: TransferProgress::Unknown,
                });
            }
        }
    }
    fn snapshot(&self) -> HcdSnapshot {
        self.snapshot
    }
    fn requires_foreground_polling(&self) -> bool {
        // Without the ISR every completion has to be found by reading HCINT.
        // With it, the only thing the core cannot do on its own is the
        // software half of a split transaction, whose 125 us phase delay is
        // far shorter than the system tick that would otherwise wake us.
        !interrupt::is_active()
            || self
                .slots
                .iter()
                .any(|slot| slot.active && slot.split_pending)
    }
}

pub use interrupt::{handle as interrupt_handler, install as install_interrupt};
