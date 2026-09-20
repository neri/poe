use libusb::{
    DataPid, Direction, EndpointAddress, TransferProgress, TransferType, UsbAddress, UsbError,
    UsbSpeed,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TransferToken {
    slot: u8,
    generation: u32,
}
impl TransferToken {
    pub const fn new(slot: u8, generation: u32) -> Self {
        Self { slot, generation }
    }
    pub const fn slot(self) -> u8 {
        self.slot
    }
    pub const fn generation(self) -> u32 {
        self.generation
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SplitTarget {
    pub hub_address: UsbAddress,
    pub port_number: u8,
    pub hub_speed: UsbSpeed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UsbRoute {
    pub device_speed: UsbSpeed,
    pub translator: Option<SplitTarget>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RootPortState {
    Disconnected,
    Connected(UsbSpeed),
    Enabled(UsbSpeed),
    Fault,
}

#[derive(Debug)]
pub struct TransferRequest<'a> {
    pub address: UsbAddress,
    pub endpoint: EndpointAddress,
    pub transfer_type: TransferType,
    pub direction: Direction,
    pub route: UsbRoute,
    pub max_packet_size: u16,
    pub pid: DataPid,
    pub buffer: &'a mut [u8],
    pub deadline_us: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TransferCompletion {
    pub token: TransferToken,
    pub result: Result<usize, UsbError>,
    pub progress: TransferProgress,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct HcdSnapshot {
    pub submitted: u64,
    pub reaped: u64,
    pub cancelled: u64,
    pub active: u8,
    pub stale_tokens: u64,
    pub last_interrupt: u32,
    /// Per-reason completion counts. A NAK or NYET is a normal answer, not a
    /// fault, and is counted separately from the rest for that reason.
    pub completed: u64,
    pub naks: u64,
    pub nyets: u64,
    pub stalls: u64,
    pub timeouts: u64,
    pub transaction_errors: u64,
    pub other_errors: u64,
    /// Split-transaction outcomes. `split_exhausted` is the one that loses
    /// data: the translator had already run the low-speed transaction, so the
    /// payload it was holding is gone when the host stops collecting it.
    pub split_csplit_retries: u64,
    pub split_exhausted: u64,
    pub split_naks: u64,
    /// Start-splits held back to the next frame. Kept because the loss rate
    /// was measured to follow this, not the frame boundary.
    pub split_deferrals: u64,
    /// Periodic split transfers started in each microframe, and how many of
    /// them failed to collect. Whether a poll succeeds turned out to depend on
    /// which microframe it starts in, so the two are counted side by side.
    /// Periodic splits abandoned at the frame boundary, where the translator
    /// has already let the transaction go. Distinct from `split_exhausted`,
    /// which is the last-resort exit that leaves it holding one.
    pub split_expired: u64,
    pub periodic_starts: [u32; 8],
    pub periodic_losses: [u32; 8],
}

pub trait HostController {
    fn root_port_state(&self) -> RootPortState;
    fn reset_root_port(&mut self, deadline_us: u64) -> Result<(), UsbError>;
    fn submit(&mut self, request: TransferRequest<'_>) -> Result<TransferToken, UsbError>;
    fn cancel(&mut self, token: TransferToken) -> Result<TransferProgress, UsbError>;
    fn reap(&mut self) -> Option<TransferCompletion>;
    fn service_timeouts(&mut self, now_us: u64);
    fn snapshot(&self) -> HcdSnapshot;

    /// Returns true while the controller cannot make progress on its own.
    ///
    /// A controller whose completions arrive as interrupts returns false, so
    /// the system may sleep in `WFI` between transfers.  It must return true
    /// again whenever it is waiting on a software-scheduled deadline that is
    /// shorter than the system tick, such as a split-transaction retry.
    fn requires_foreground_polling(&self) -> bool {
        true
    }
}
