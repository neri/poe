//! Transfer Request Blocks and the rings made of them.
//!
//! A ring is a fixed array of TRBs whose last entry is a Link TRB pointing
//! back at the start.  Ownership is carried by the Cycle Bit: the producer
//! writes TRBs with the current cycle and flips it each time it wraps, and
//! the consumer stops as soon as it reads a TRB whose cycle does not match.

/// A Transfer Request Block: four 32-bit fields, 16-byte aligned.
#[repr(C, align(16))]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Trb {
    pub parameter: u64,
    pub status: u32,
    pub control: u32,
}

impl Trb {
    pub const SIZE: usize = core::mem::size_of::<Self>();

    #[inline]
    pub const fn cycle(&self) -> bool {
        self.control & 1 != 0
    }

    #[inline]
    pub const fn trb_type(&self) -> u8 {
        ((self.control >> 10) & 0x3f) as u8
    }

    /// The completion code of an event TRB.
    #[inline]
    pub const fn completion_code(&self) -> u8 {
        ((self.status >> 24) & 0xff) as u8
    }

    /// The residual byte count of a transfer event.
    #[inline]
    pub const fn transfer_length(&self) -> u32 {
        self.status & 0x1f_ffff
    }

    /// The slot ID of a command completion or transfer event.
    #[inline]
    pub const fn slot_id(&self) -> u8 {
        (self.control >> 24) as u8
    }

    /// The endpoint device context index of a transfer event.
    #[inline]
    pub const fn endpoint_id(&self) -> u8 {
        ((self.control >> 16) & 0x1f) as u8
    }

    /// The root hub port number of a Port Status Change event.
    #[inline]
    pub const fn port_id(&self) -> u8 {
        ((self.parameter >> 24) & 0xff) as u8
    }
}

/// TRB type codes.
pub mod trb_type {
    // Transfer ring
    pub const NORMAL: u8 = 1;
    pub const SETUP_STAGE: u8 = 2;
    pub const DATA_STAGE: u8 = 3;
    pub const STATUS_STAGE: u8 = 4;
    pub const LINK: u8 = 6;
    pub const EVENT_DATA: u8 = 7;
    pub const NO_OP: u8 = 8;

    // Command ring
    pub const ENABLE_SLOT: u8 = 9;
    pub const DISABLE_SLOT: u8 = 10;
    pub const ADDRESS_DEVICE: u8 = 11;
    pub const CONFIGURE_ENDPOINT: u8 = 12;
    pub const EVALUATE_CONTEXT: u8 = 13;
    pub const RESET_ENDPOINT: u8 = 14;
    pub const STOP_ENDPOINT: u8 = 15;
    pub const SET_TR_DEQUEUE_POINTER: u8 = 16;
    pub const RESET_DEVICE: u8 = 17;
    pub const NO_OP_COMMAND: u8 = 23;

    // Event ring
    pub const TRANSFER_EVENT: u8 = 32;
    pub const COMMAND_COMPLETION_EVENT: u8 = 33;
    pub const PORT_STATUS_CHANGE_EVENT: u8 = 34;
    pub const BANDWIDTH_REQUEST_EVENT: u8 = 35;
    pub const DOORBELL_EVENT: u8 = 36;
    pub const HOST_CONTROLLER_EVENT: u8 = 37;
    pub const DEVICE_NOTIFICATION_EVENT: u8 = 38;
    pub const MFINDEX_WRAP_EVENT: u8 = 39;
}

/// TRB control bits shared by several types.
pub mod trb_flags {
    pub const CYCLE: u32 = 1 << 0;
    pub const TOGGLE_CYCLE: u32 = 1 << 1;
    pub const INTERRUPT_ON_SHORT_PACKET: u32 = 1 << 2;
    pub const NO_SNOOP: u32 = 1 << 3;
    pub const CHAIN: u32 = 1 << 4;
    pub const INTERRUPT_ON_COMPLETION: u32 = 1 << 5;
    pub const IMMEDIATE_DATA: u32 = 1 << 6;
    /// Data Stage / Status Stage direction: set for IN.
    pub const DIRECTION_IN: u32 = 1 << 16;
    /// Address Device: set the address but do not issue SET_ADDRESS.
    pub const BLOCK_SET_ADDRESS: u32 = 1 << 9;

    #[inline]
    pub const fn trb_type(value: u8) -> u32 {
        (value as u32) << 10
    }
}

/// Completion codes reported in event TRBs.
pub mod completion {
    pub const INVALID: u8 = 0;
    pub const SUCCESS: u8 = 1;
    pub const DATA_BUFFER_ERROR: u8 = 2;
    pub const BABBLE_DETECTED: u8 = 3;
    pub const USB_TRANSACTION_ERROR: u8 = 4;
    pub const TRB_ERROR: u8 = 5;
    pub const STALL_ERROR: u8 = 6;
    pub const RESOURCE_ERROR: u8 = 7;
    pub const BANDWIDTH_ERROR: u8 = 8;
    pub const NO_SLOTS_AVAILABLE: u8 = 9;
    pub const SLOT_NOT_ENABLED: u8 = 11;
    pub const ENDPOINT_NOT_ENABLED: u8 = 12;
    pub const SHORT_PACKET: u8 = 13;
    pub const RING_UNDERRUN: u8 = 14;
    pub const RING_OVERRUN: u8 = 15;
    pub const PARAMETER_ERROR: u8 = 17;
    pub const CONTEXT_STATE_ERROR: u8 = 19;
    pub const NO_PING_RESPONSE: u8 = 20;
    pub const EVENT_RING_FULL: u8 = 21;
    pub const COMMAND_RING_STOPPED: u8 = 24;
    pub const COMMAND_ABORTED: u8 = 25;
    pub const STOPPED: u8 = 26;
    pub const STOPPED_LENGTH_INVALID: u8 = 27;
}

/// Maps a completion code to the error the rest of the USB stack speaks.
pub fn completion_to_error(code: u8) -> Result<(), libusb::UsbError> {
    use libusb::UsbError;
    match code {
        completion::SUCCESS | completion::SHORT_PACKET => Ok(()),
        completion::STALL_ERROR => Err(UsbError::Stall),
        completion::BABBLE_DETECTED => Err(UsbError::Babble),
        completion::USB_TRANSACTION_ERROR | completion::NO_PING_RESPONSE => {
            Err(UsbError::Transaction)
        }
        completion::DATA_BUFFER_ERROR | completion::RING_UNDERRUN | completion::RING_OVERRUN => {
            Err(UsbError::Buffer)
        }
        completion::RESOURCE_ERROR
        | completion::BANDWIDTH_ERROR
        | completion::NO_SLOTS_AVAILABLE
        | completion::EVENT_RING_FULL => Err(UsbError::ResourceExhausted),
        completion::COMMAND_ABORTED
        | completion::COMMAND_RING_STOPPED
        | completion::STOPPED
        | completion::STOPPED_LENGTH_INVALID => Err(UsbError::Cancelled),
        completion::SLOT_NOT_ENABLED | completion::ENDPOINT_NOT_ENABLED => {
            Err(UsbError::Disconnected)
        }
        // The command or transfer did not apply in the state the slot or
        // endpoint was actually in.  Distinct from a controller fault: it
        // says the caller's idea of the state was stale, which a recovery
        // path wants to know rather than treat as fatal.
        completion::CONTEXT_STATE_ERROR | completion::PARAMETER_ERROR => {
            Err(UsbError::InvalidRequest)
        }
        _ => Err(UsbError::ControllerFault),
    }
}

/// One segment of an Event Ring Segment Table.
#[repr(C, align(16))]
#[derive(Clone, Copy, Debug, Default)]
pub struct ErstEntry {
    pub ring_segment_base: u64,
    /// The segment's size in TRBs, in the low 16 bits.
    pub ring_segment_size: u32,
    pub reserved: u32,
}

/// Producer-side bookkeeping for a command or transfer ring.
///
/// The ring memory itself is owned by the caller; this only tracks where the
/// next TRB goes and which cycle bit it carries.  The last entry of the ring
/// is reserved for the Link TRB, so a ring of `len` TRBs holds `len - 1`.
#[derive(Clone, Copy, Debug)]
pub struct RingState {
    len: usize,
    enqueue: usize,
    cycle: bool,
}

impl RingState {
    pub const fn new(len: usize) -> Self {
        Self {
            len,
            enqueue: 0,
            cycle: true,
        }
    }

    #[inline]
    pub const fn len(&self) -> usize {
        self.len
    }

    /// The index the next TRB will be written to.
    #[inline]
    pub const fn enqueue_index(&self) -> usize {
        self.enqueue
    }

    #[inline]
    pub const fn cycle(&self) -> bool {
        self.cycle
    }

    /// The Link TRB that closes the ring, with the current cycle bit.
    ///
    /// `Toggle Cycle` is set so the controller flips its own cycle state at
    /// the same point the producer does.
    pub fn link_trb(&self, base: u64) -> Trb {
        Trb {
            parameter: base,
            status: 0,
            control: trb_flags::trb_type(trb_type::LINK)
                | trb_flags::TOGGLE_CYCLE
                | if self.cycle { trb_flags::CYCLE } else { 0 },
        }
    }

    /// Advances past the TRB just written, and reports whether the producer
    /// has reached the Link TRB and has to rewrite it and wrap.
    ///
    /// The caller writes the Link TRB returned by [`Self::link_trb`] with the
    /// *old* cycle before wrapping, which is what hands the segment back.
    #[must_use]
    pub fn advance(&mut self) -> Option<LinkWrap> {
        self.enqueue += 1;
        if self.enqueue + 1 < self.len {
            return None;
        }
        let link_index = self.len - 1;
        let link_cycle = self.cycle;
        self.enqueue = 0;
        self.cycle = !self.cycle;
        Some(LinkWrap {
            link_index,
            link_cycle,
        })
    }
}

/// What the caller must do when the producer reaches the end of a ring.
#[derive(Clone, Copy, Debug)]
pub struct LinkWrap {
    /// Index of the Link TRB to publish.
    pub link_index: usize,
    /// Cycle bit the Link TRB must carry to transfer ownership.
    pub link_cycle: bool,
}

/// Consumer-side bookkeeping for the event ring.
#[derive(Clone, Copy, Debug)]
pub struct EventRingState {
    len: usize,
    dequeue: usize,
    cycle: bool,
}

impl EventRingState {
    pub const fn new(len: usize) -> Self {
        Self {
            len,
            dequeue: 0,
            cycle: true,
        }
    }

    #[inline]
    pub const fn dequeue_index(&self) -> usize {
        self.dequeue
    }

    /// True if `trb` belongs to the consumer, i.e. the controller has written it.
    #[inline]
    pub const fn owns(&self, trb: &Trb) -> bool {
        trb.cycle() == self.cycle
    }

    /// Steps past a consumed event.  The event ring has no Link TRB: the
    /// consumer wraps at the end of the segment and flips its own cycle.
    pub fn advance(&mut self) {
        self.dequeue += 1;
        if self.dequeue >= self.len {
            self.dequeue = 0;
            self.cycle = !self.cycle;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_ring_wraps_after_its_last_usable_slot_and_flips_the_cycle() {
        // Four TRBs: three usable, the fourth is the Link.
        let mut ring = RingState::new(4);
        assert!(ring.cycle());
        for expected in 0..2 {
            assert_eq!(ring.enqueue_index(), expected);
            assert!(ring.advance().is_none());
        }
        assert_eq!(ring.enqueue_index(), 2);
        let wrap = ring.advance().expect("the third TRB fills the segment");
        assert_eq!(wrap.link_index, 3);
        // The Link TRB is published with the cycle the segment was filled
        // with, not the new one, or the controller would never run it.
        assert!(wrap.link_cycle);
        assert_eq!(ring.enqueue_index(), 0);
        assert!(!ring.cycle());
    }

    #[test]
    fn the_event_ring_consumer_flips_its_cycle_on_wrap() {
        let mut events = EventRingState::new(2);
        let owned = Trb {
            control: trb_flags::CYCLE,
            ..Default::default()
        };
        let stale = Trb::default();
        assert!(events.owns(&owned));
        assert!(!events.owns(&stale));
        events.advance();
        events.advance();
        assert_eq!(events.dequeue_index(), 0);
        // After one lap the controller writes zero-cycle TRBs.
        assert!(!events.owns(&owned));
        assert!(events.owns(&stale));
    }

    #[test]
    fn a_short_packet_is_not_an_error() {
        assert!(completion_to_error(completion::SHORT_PACKET).is_ok());
        assert_eq!(
            completion_to_error(completion::STALL_ERROR),
            Err(libusb::UsbError::Stall)
        );
    }

    #[test]
    fn a_context_state_error_is_told_apart_from_a_controller_fault() {
        // Recovery issues Reset Endpoint against an endpoint that may have
        // already been reset, and has to tell "that did not apply" from
        // "the controller is broken".
        assert_eq!(
            completion_to_error(completion::CONTEXT_STATE_ERROR),
            Err(libusb::UsbError::InvalidRequest)
        );
        assert_eq!(
            completion_to_error(completion::INVALID),
            Err(libusb::UsbError::ControllerFault)
        );
    }

    #[test]
    fn event_trb_fields_are_decoded_from_the_right_bits() {
        let event = Trb {
            parameter: 0x03 << 24,
            status: (completion::SUCCESS as u32) << 24 | 7,
            control: trb_flags::trb_type(trb_type::TRANSFER_EVENT) | (2 << 16) | (5 << 24),
        };
        assert_eq!(event.trb_type(), trb_type::TRANSFER_EVENT);
        assert_eq!(event.completion_code(), completion::SUCCESS);
        assert_eq!(event.transfer_length(), 7);
        assert_eq!(event.endpoint_id(), 2);
        assert_eq!(event.slot_id(), 5);
        assert_eq!(event.port_id(), 3);
    }
}
