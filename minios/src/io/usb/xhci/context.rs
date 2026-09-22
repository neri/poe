//! Slot, Endpoint and Input contexts.
//!
//! The controller decides at runtime whether a context is 32 or 64 bytes
//! (`HCCPARAMS1.CSZ`), so these are addressed as 32-bit words at a computed
//! stride rather than as Rust structs.  A context is a block of DMA memory;
//! `ContextLayout` turns a (context index, field index) pair into an offset
//! into it.

use libusb::{Direction, TransferType, UsbSpeed};

/// The device context index of a control endpoint (EP0), and the base from
/// which the other endpoints are numbered: `DCI = 2 * n + direction`.
pub const DCI_CONTROL: u8 = 1;

/// Turns an endpoint number and direction into a device context index.
#[inline]
pub const fn device_context_index(number: u8, direction: Direction) -> u8 {
    if number == 0 {
        return DCI_CONTROL;
    }
    2 * number
        + match direction {
            Direction::In => 1,
            Direction::Out => 0,
        }
}

/// Endpoint Context `EP Type` values.
#[inline]
pub const fn endpoint_type(transfer: TransferType, direction: Direction) -> u8 {
    let out = match transfer {
        TransferType::Control => return 4,
        TransferType::Isochronous => 1,
        TransferType::Bulk => 2,
        TransferType::Interrupt => 3,
    };
    match direction {
        Direction::In => out + 4,
        Direction::Out => out,
    }
}

/// The `Speed` field of a Slot Context, using the default speed IDs of the
/// specification.  A controller may redefine these through its Supported
/// Protocol capability; every controller this targets uses the defaults.
#[inline]
pub const fn slot_speed(speed: UsbSpeed) -> u32 {
    match speed {
        UsbSpeed::Full => 1,
        UsbSpeed::Low => 2,
        UsbSpeed::High => 3,
        UsbSpeed::Super => 4,
    }
}

/// Maps a `PORTSC` port speed ID back to a USB speed.
///
/// SuperSpeed (4) is driven.  SuperSpeedPlus (5 and up) is reported as `None`
/// rather than folded into SuperSpeed: its Slot Context speed and packet
/// rules differ, and a controller that has it is not one this targets.  The
/// VL805 is USB 3.0 only.
#[inline]
pub const fn speed_from_port(id: u8) -> Option<UsbSpeed> {
    match id {
        1 => Some(UsbSpeed::Full),
        2 => Some(UsbSpeed::Low),
        3 => Some(UsbSpeed::High),
        4 => Some(UsbSpeed::Super),
        _ => None,
    }
}

/// The default maximum packet size of EP0 before the device descriptor has
/// been read.  Low Speed is fixed at 8; Full Speed may be 8, 16, 32 or 64 and
/// is corrected with an Evaluate Context once the first eight bytes arrive.
/// SuperSpeed is fixed at 512.
#[inline]
pub const fn default_max_packet_size(speed: UsbSpeed) -> u16 {
    match speed {
        UsbSpeed::Low => 8,
        UsbSpeed::Full => 8,
        UsbSpeed::High => 64,
        UsbSpeed::Super => 512,
    }
}

/// EP0's packet size from `bMaxPacketSize0` of a device descriptor.
///
/// From USB 3.0 on the field is an exponent (9 for 512 bytes) when the device
/// runs at SuperSpeed, and a byte count otherwise.  `None` for a value that
/// is not valid at `speed`.
#[inline]
pub const fn ep0_packet_size(speed: UsbSpeed, b_max_packet_size_0: u8) -> Option<u16> {
    match (speed, b_max_packet_size_0) {
        (UsbSpeed::Super, 9) => Some(512),
        (UsbSpeed::Super, _) => None,
        (_, size @ (8 | 16 | 32 | 64)) => Some(size as u16),
        _ => None,
    }
}

/// Where each context sits inside a device or input context block.
#[derive(Clone, Copy, Debug)]
pub struct ContextLayout {
    /// 32 or 64 bytes.
    stride: usize,
}

impl ContextLayout {
    pub const fn new(context_bytes: usize) -> Self {
        Self {
            stride: context_bytes,
        }
    }

    #[inline]
    pub const fn stride(&self) -> usize {
        self.stride
    }

    /// Bytes in a Device Context: a Slot Context plus 31 Endpoint Contexts.
    #[inline]
    pub const fn device_context_bytes(&self) -> usize {
        self.stride * 32
    }

    /// Bytes in an Input Context: an Input Control Context ahead of a full
    /// Device Context.
    #[inline]
    pub const fn input_context_bytes(&self) -> usize {
        self.stride * 33
    }

    /// Word offset of field `word` of the Input Control Context.
    #[inline]
    pub const fn input_control_word(&self, word: usize) -> usize {
        word
    }

    /// Word offset of field `word` of the Slot Context inside an *input* context.
    #[inline]
    pub const fn input_slot_word(&self, word: usize) -> usize {
        self.stride / 4 + word
    }

    /// Word offset of field `word` of endpoint `dci` inside an *input* context.
    #[inline]
    pub const fn input_endpoint_word(&self, dci: u8, word: usize) -> usize {
        (self.stride / 4) * (1 + dci as usize) + word
    }

    /// Word offset of field `word` of the Slot Context inside a *device* context.
    #[inline]
    pub const fn device_slot_word(&self, word: usize) -> usize {
        word
    }

    /// Word offset of field `word` of endpoint `dci` inside a *device* context.
    #[inline]
    pub const fn device_endpoint_word(&self, dci: u8, word: usize) -> usize {
        (self.stride / 4) * dci as usize + word
    }
}

/// The fields of a Slot Context this driver sets, gathered so the bit packing
/// lives in one place.
#[derive(Clone, Copy, Debug, Default)]
pub struct SlotContextFields {
    /// Route String: four bits per hub tier, zero for a root port device.
    pub route_string: u32,
    pub speed: u32,
    /// `MTT`: the parent hub has a multi-TT translator in use.
    pub multi_tt: bool,
    /// `Hub`: this device is itself a hub.
    pub hub: bool,
    /// The highest device context index in use.
    pub context_entries: u8,
    pub root_hub_port: u8,
    /// Number of downstream ports, for a hub.
    pub number_of_ports: u8,
    /// Slot ID of the parent High Speed hub, for a Low/Full Speed device
    /// behind it, and the port it is on.
    pub tt_hub_slot: u8,
    pub tt_port: u8,
    /// `TT Think Time`, as encoded in the hub descriptor.
    pub tt_think_time: u8,
    pub interrupter: u16,
}

impl SlotContextFields {
    /// The four Slot Context words, in order.
    pub const fn words(&self) -> [u32; 4] {
        let dw0 = (self.route_string & 0x000f_ffff)
            | (self.speed & 0xf) << 20
            | (self.multi_tt as u32) << 25
            | (self.hub as u32) << 26
            | (self.context_entries as u32) << 27;
        // Max Exit Latency stays zero: this driver never suspends a link, so
        // the controller has no resume path to budget for.
        let dw1 = ((self.root_hub_port as u32) << 16) | ((self.number_of_ports as u32) << 24);
        let dw2 = (self.tt_hub_slot as u32)
            | ((self.tt_port as u32) << 8)
            | ((self.tt_think_time as u32 & 3) << 16)
            | ((self.interrupter as u32 & 0x3ff) << 22);
        // USB Device Address and Slot State are written by the controller.
        [dw0, dw1, dw2, 0]
    }
}

/// The fields of an Endpoint Context this driver sets.
#[derive(Clone, Copy, Debug)]
pub struct EndpointContextFields {
    pub endpoint_type: u8,
    pub max_packet_size: u16,
    pub max_burst_size: u8,
    /// `Interval`: the polling period as a power of two in 125 µs units.
    pub interval: u8,
    /// `CErr`: how many times the controller retries a failed transaction.
    /// Three is the value the specification prescribes for everything but
    /// isochronous endpoints.
    pub error_count: u8,
    pub dequeue_pointer: u64,
    pub dequeue_cycle: bool,
    pub average_trb_length: u16,
    pub max_esit_payload: u16,
}

impl EndpointContextFields {
    /// A control endpoint with the given packet size and transfer ring.
    pub const fn control(max_packet_size: u16, dequeue_pointer: u64) -> Self {
        Self {
            endpoint_type: 4,
            max_packet_size,
            max_burst_size: 0,
            interval: 0,
            error_count: 3,
            dequeue_pointer,
            dequeue_cycle: true,
            // The specification asks for the expected TRB length; 8 is the
            // Setup packet, which is the only stage always present.
            average_trb_length: 8,
            max_esit_payload: 0,
        }
    }

    /// The first five Endpoint Context words, in order.  The remaining three
    /// are reserved and left zero.
    pub const fn words(&self) -> [u32; 5] {
        let dw0 = ((self.interval as u32) << 16) | ((self.max_esit_payload as u32 >> 16) << 24);
        let dw1 = ((self.error_count as u32 & 3) << 1)
            | ((self.endpoint_type as u32 & 7) << 3)
            | ((self.max_burst_size as u32) << 8)
            | ((self.max_packet_size as u32) << 16);
        let dequeue = self.dequeue_pointer | self.dequeue_cycle as u64;
        let dw2 = dequeue as u32;
        let dw3 = (dequeue >> 32) as u32;
        let dw4 =
            (self.average_trb_length as u32) | ((self.max_esit_payload as u32 & 0xffff) << 16);
        [dw0, dw1, dw2, dw3, dw4]
    }
}

/// Converts a `bInterval` from an endpoint descriptor into the `Interval`
/// field of an Endpoint Context.
///
/// The two are encoded differently by speed.  For High Speed and SuperSpeed
/// interrupt endpoints `bInterval` is already a power-of-two exponent of
/// 125 µs frames.
/// For Low and Full Speed it is a frame count in milliseconds, which has to
/// become the exponent of the largest power of two that does not exceed it,
/// shifted by three because a frame is eight 125 µs intervals.
pub fn interrupt_interval(speed: UsbSpeed, b_interval: u8) -> u8 {
    match speed {
        UsbSpeed::High | UsbSpeed::Super => b_interval.clamp(1, 16) - 1,
        UsbSpeed::Low | UsbSpeed::Full => {
            let frames = b_interval.max(1) as u32;
            // floor(log2(frames)) + 3, clamped to the encodable range.
            let exponent = 31 - frames.leading_zeros();
            (exponent + 3).min(15) as u8
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_context_indices_follow_the_specification() {
        assert_eq!(device_context_index(0, Direction::Out), 1);
        assert_eq!(device_context_index(0, Direction::In), 1);
        assert_eq!(device_context_index(1, Direction::Out), 2);
        assert_eq!(device_context_index(1, Direction::In), 3);
        assert_eq!(device_context_index(15, Direction::In), 31);
    }

    #[test]
    fn endpoint_types_separate_the_two_directions() {
        assert_eq!(endpoint_type(TransferType::Control, Direction::In), 4);
        assert_eq!(endpoint_type(TransferType::Interrupt, Direction::Out), 3);
        assert_eq!(endpoint_type(TransferType::Interrupt, Direction::In), 7);
        assert_eq!(endpoint_type(TransferType::Bulk, Direction::In), 6);
    }

    #[test]
    fn a_64_byte_context_doubles_every_stride() {
        let small = ContextLayout::new(32);
        let large = ContextLayout::new(64);
        assert_eq!(small.device_context_bytes(), 1024);
        assert_eq!(large.device_context_bytes(), 2048);
        assert_eq!(small.input_context_bytes(), 1056);
        // The Slot Context of an input context sits behind the control context.
        assert_eq!(small.input_slot_word(0), 8);
        assert_eq!(large.input_slot_word(0), 16);
        // EP0 is the second context of the device context, the third of the input.
        assert_eq!(small.device_endpoint_word(DCI_CONTROL, 0), 8);
        assert_eq!(small.input_endpoint_word(DCI_CONTROL, 0), 16);
    }

    #[test]
    fn slot_context_packs_speed_and_port_where_the_controller_reads_them() {
        let words = SlotContextFields {
            speed: slot_speed(UsbSpeed::Low),
            context_entries: 1,
            root_hub_port: 3,
            ..Default::default()
        }
        .words();
        assert_eq!(words[0] >> 20 & 0xf, 2);
        assert_eq!(words[0] >> 27, 1);
        assert_eq!(words[1] >> 16 & 0xff, 3);
    }

    #[test]
    fn an_endpoint_context_carries_the_dequeue_cycle_in_bit_zero() {
        let fields = EndpointContextFields::control(8, 0x1234_5000);
        let words = fields.words();
        assert_eq!(words[1] >> 16, 8);
        assert_eq!(words[1] >> 3 & 7, 4);
        assert_eq!(words[2], 0x1234_5001);
        assert_eq!(words[3], 0);
    }

    #[test]
    fn interrupt_intervals_convert_by_speed() {
        // High Speed: bInterval is already an exponent, one-based.
        assert_eq!(interrupt_interval(UsbSpeed::High, 1), 0);
        assert_eq!(interrupt_interval(UsbSpeed::High, 4), 3);
        // Low/Full Speed: a 10 ms poll rounds down to 8 ms = 2^3 frames = 2^6
        // microframes.
        assert_eq!(interrupt_interval(UsbSpeed::Full, 10), 6);
        assert_eq!(interrupt_interval(UsbSpeed::Low, 1), 3);
        // A device reporting zero must not produce a shift of -1.
        assert_eq!(interrupt_interval(UsbSpeed::Full, 0), 3);
        // SuperSpeed is encoded as High Speed.
        assert_eq!(interrupt_interval(UsbSpeed::Super, 4), 3);
    }

    #[test]
    fn superspeed_is_speed_id_four_with_a_512_byte_ep0() {
        assert_eq!(speed_from_port(4), Some(UsbSpeed::Super));
        assert_eq!(slot_speed(UsbSpeed::Super), 4);
        assert_eq!(default_max_packet_size(UsbSpeed::Super), 512);
        // SuperSpeedPlus is not folded into SuperSpeed.
        assert_eq!(speed_from_port(5), None);
    }

    #[test]
    fn ep0_packet_size_is_an_exponent_only_at_superspeed() {
        assert_eq!(ep0_packet_size(UsbSpeed::Super, 9), Some(512));
        assert_eq!(ep0_packet_size(UsbSpeed::Super, 64), None);
        assert_eq!(ep0_packet_size(UsbSpeed::High, 64), Some(64));
        assert_eq!(ep0_packet_size(UsbSpeed::Full, 8), Some(8));
        assert_eq!(ep0_packet_size(UsbSpeed::Full, 9), None);
    }
}
