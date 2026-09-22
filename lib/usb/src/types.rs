use core::fmt;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd)]
#[repr(transparent)]
pub struct UsbAddress(u8);

impl UsbAddress {
    pub const DEFAULT: Self = Self(0);

    pub const fn new(value: u8) -> Option<Self> {
        if value <= 127 {
            Some(Self(value))
        } else {
            None
        }
    }

    pub const fn get(self) -> u8 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UsbSpeed {
    Low,
    Full,
    High,
    /// USB 3.x SuperSpeed (5 Gb/s) or faster.  Only an xHCI controller
    /// reaches it; the USB 2.0 paths never see this value.
    Super,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Direction {
    Out,
    In,
}

#[derive(Clone, Copy, Eq, PartialEq)]
#[repr(transparent)]
pub struct EndpointAddress(u8);

impl EndpointAddress {
    pub const fn new(number: u8, direction: Direction) -> Option<Self> {
        if number > 15 {
            return None;
        }
        Some(Self(
            number
                | match direction {
                    Direction::Out => 0,
                    Direction::In => 0x80,
                },
        ))
    }
    pub const fn from_raw(value: u8) -> Option<Self> {
        if value & 0x70 == 0 {
            Some(Self(value))
        } else {
            None
        }
    }
    pub const fn number(self) -> u8 {
        self.0 & 0x0f
    }
    pub const fn direction(self) -> Direction {
        if self.0 & 0x80 != 0 {
            Direction::In
        } else {
            Direction::Out
        }
    }
    pub const fn raw(self) -> u8 {
        self.0
    }
}

impl fmt::Debug for EndpointAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "EndpointAddress({} {:?})",
            self.number(),
            self.direction()
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransferType {
    Control,
    Isochronous,
    Bulk,
    Interrupt,
}

impl TransferType {
    pub const fn from_attributes(value: u8) -> Self {
        match value & 3 {
            0 => Self::Control,
            1 => Self::Isochronous,
            2 => Self::Bulk,
            _ => Self::Interrupt,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UsbError {
    Nak,
    Nyet,
    Disconnected,
    Stall,
    Timeout,
    Transaction,
    Babble,
    Buffer,
    Cancelled,
    InvalidDescriptor,
    InvalidRequest,
    ResourceExhausted,
    Unsupported,
    ControllerFault,
    Dma,
}
