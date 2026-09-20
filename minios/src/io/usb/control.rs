use libusb::{
    DataPid, Direction, EndpointAddress, SetupPacket, TransferType, UsbAddress, UsbError, UsbSpeed,
};

use super::hcd::{HostController, TransferCompletion, TransferRequest, TransferToken, UsbRoute};

const CONTROL_TIMEOUT_US: u64 = 1_000_000;
const MAX_RETRIES: u8 = 8;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Stage {
    Idle,
    Setup,
    Data,
    Status,
}

pub struct ControlTransfer {
    stage: Stage,
    failure_stage: Stage,
    token: Option<TransferToken>,
    setup: SetupPacket,
    setup_bytes: [u8; 8],
    status: [u8; 0],
    pub data: [u8; 512],
    actual: usize,
    data_chunk_len: usize,
    data_pid: DataPid,
    address: UsbAddress,
    speed: UsbSpeed,
    max_packet_size: u16,
    retries: u8,
    route: UsbRoute,
}

#[derive(Clone, Copy)]
pub struct ControlRequestContext {
    pub address: UsbAddress,
    pub route: UsbRoute,
    pub max_packet_size: u16,
    pub setup: SetupPacket,
}

impl ControlTransfer {
    pub const fn new() -> Self {
        Self {
            stage: Stage::Idle,
            failure_stage: Stage::Idle,
            token: None,
            setup: SetupPacket::new(0, 0, 0, 0, 0),
            setup_bytes: [0; 8],
            status: [],
            data: [0; 512],
            actual: 0,
            data_chunk_len: 0,
            data_pid: DataPid::Data1,
            address: UsbAddress::DEFAULT,
            speed: UsbSpeed::Full,
            max_packet_size: 8,
            retries: 0,
            route: UsbRoute {
                device_speed: UsbSpeed::Full,
                translator: None,
            },
        }
    }

    pub fn is_active(&self) -> bool {
        self.stage != Stage::Idle
    }

    pub fn owns(&self, token: TransferToken) -> bool {
        self.token == Some(token)
    }

    pub fn failure_context(&self) -> (&'static str, SetupPacket, usize, u8) {
        let stage = match self.failure_stage {
            Stage::Idle => "Idle",
            Stage::Setup => "Setup",
            Stage::Data => "Data",
            Stage::Status => "Status",
        };
        (stage, self.setup, self.actual, self.retries)
    }

    pub const fn request_context(&self) -> ControlRequestContext {
        ControlRequestContext {
            address: self.address,
            route: self.route,
            max_packet_size: self.max_packet_size,
            setup: self.setup,
        }
    }

    pub fn cancel(&mut self, hcd: &mut dyn HostController) {
        if let Some(token) = self.token.take() {
            let _ = hcd.cancel(token);
        }
        self.stage = Stage::Idle;
        self.actual = 0;
        self.data_chunk_len = 0;
        self.data_pid = DataPid::Data1;
        self.failure_stage = Stage::Idle;
        self.retries = 0;
    }

    pub fn start(
        &mut self,
        hcd: &mut dyn HostController,
        address: UsbAddress,
        speed: UsbSpeed,
        max_packet_size: u16,
        setup: SetupPacket,
        now_us: u64,
    ) -> Result<(), UsbError> {
        self.start_routed(
            hcd,
            address,
            UsbRoute {
                device_speed: speed,
                translator: None,
            },
            max_packet_size,
            setup,
            now_us,
        )
    }

    pub fn start_routed(
        &mut self,
        hcd: &mut dyn HostController,
        address: UsbAddress,
        route: UsbRoute,
        max_packet_size: u16,
        setup: SetupPacket,
        now_us: u64,
    ) -> Result<(), UsbError> {
        if self.is_active() || setup.length as usize > self.data.len() {
            return Err(UsbError::InvalidRequest);
        }
        self.setup = setup;
        self.setup_bytes = setup.to_bytes();
        self.address = address;
        self.speed = route.device_speed;
        self.route = route;
        self.max_packet_size = max_packet_size;
        self.actual = 0;
        self.retries = 0;
        // Every control transfer starts its data stage with DATA1.  The
        // toggle is per-transfer, not per-endpoint, so it must not be
        // inherited from the previous request on this (shared) transfer
        // object: doing so makes the first data packet arrive with the wrong
        // PID and the device reports a data-toggle error.
        self.data_chunk_len = 0;
        self.data_pid = DataPid::Data1;
        self.submit_setup(hcd, now_us)
    }

    fn route(&self) -> UsbRoute {
        self.route
    }

    fn submit_setup(&mut self, hcd: &mut dyn HostController, now_us: u64) -> Result<(), UsbError> {
        self.stage = Stage::Setup;
        self.token = Some(hcd.submit(TransferRequest {
            address: self.address,
            endpoint: EndpointAddress::new(0, Direction::Out).unwrap(),
            transfer_type: TransferType::Control,
            direction: Direction::Out,
            route: self.route(),
            max_packet_size: self.max_packet_size,
            pid: DataPid::Setup,
            buffer: &mut self.setup_bytes,
            deadline_us: now_us.saturating_add(CONTROL_TIMEOUT_US),
        })?);
        Ok(())
    }

    fn submit_data(&mut self, hcd: &mut dyn HostController, now_us: u64) -> Result<(), UsbError> {
        let direction = self.setup.direction();
        let remaining = self.setup.length as usize - self.actual;
        // A transaction translator handles only one full-/low-speed USB
        // packet in each start-split/complete-split sequence.  Do not ask the
        // DWC2 channel to send a multi-packet transfer through a high-speed
        // hub; advance the control data stage one packet at a time instead.
        self.data_chunk_len = if self.route.translator.is_some() {
            remaining.min(self.max_packet_size as usize)
        } else {
            remaining
        };
        let end = self.actual + self.data_chunk_len;
        self.stage = Stage::Data;
        self.token = Some(hcd.submit(TransferRequest {
            address: self.address,
            endpoint: EndpointAddress::new(0, direction).unwrap(),
            transfer_type: TransferType::Control,
            direction,
            route: self.route(),
            max_packet_size: self.max_packet_size,
            pid: self.data_pid,
            buffer: &mut self.data[self.actual..end],
            deadline_us: now_us.saturating_add(CONTROL_TIMEOUT_US),
        })?);
        Ok(())
    }

    fn submit_status(&mut self, hcd: &mut dyn HostController, now_us: u64) -> Result<(), UsbError> {
        let direction = match self.setup.direction() {
            Direction::In => Direction::Out,
            Direction::Out => Direction::In,
        };
        self.stage = Stage::Status;
        self.token = Some(hcd.submit(TransferRequest {
            address: self.address,
            endpoint: EndpointAddress::new(0, direction).unwrap(),
            transfer_type: TransferType::Control,
            direction,
            route: self.route(),
            max_packet_size: self.max_packet_size,
            pid: DataPid::Data1,
            buffer: &mut self.status,
            deadline_us: now_us.saturating_add(CONTROL_TIMEOUT_US),
        })?);
        Ok(())
    }

    pub fn complete(
        &mut self,
        hcd: &mut dyn HostController,
        completion: TransferCompletion,
        now_us: u64,
    ) -> Option<Result<usize, UsbError>> {
        if self.token != Some(completion.token) {
            return None;
        }
        self.token = None;
        if let Err(error) = completion.result {
            let out_data_with_unknown_progress = self.stage == Stage::Data
                && self.setup.direction() == Direction::Out
                && completion.progress == libusb::TransferProgress::Unknown;
            let transient = matches!(error, UsbError::Nak | UsbError::Nyet)
                || (self.route.translator.is_none()
                    && matches!(error, UsbError::Transaction | UsbError::ControllerFault));
            // Reissuing an IN control request is safe even if a previous
            // attempt reached its status phase.  Never repeat an OUT data
            // stage whose progress is unknown, since that could duplicate a
            // state-changing payload.
            if transient && !out_data_with_unknown_progress && self.retries < MAX_RETRIES {
                self.retries += 1;
                self.actual = 0;
                self.data_chunk_len = 0;
                self.data_pid = DataPid::Data1;
                return self.submit_setup(hcd, now_us).err().map(Err);
            }
            self.failure_stage = self.stage;
            self.stage = Stage::Idle;
            return Some(Err(error));
        }
        let next = match self.stage {
            Stage::Setup if self.setup.length != 0 => self.submit_data(hcd, now_us),
            Stage::Setup => self.submit_status(hcd, now_us),
            Stage::Data => {
                let transferred = completion.result.unwrap_or(0);
                self.actual += transferred;
                if transferred < self.data_chunk_len || self.actual >= self.setup.length as usize {
                    self.submit_status(hcd, now_us)
                } else {
                    self.data_pid = match self.data_pid {
                        DataPid::Data0 => DataPid::Data1,
                        DataPid::Data1 => DataPid::Data0,
                        DataPid::Setup => DataPid::Data1,
                    };
                    self.submit_data(hcd, now_us)
                }
            }
            Stage::Status => {
                self.stage = Stage::Idle;
                return Some(Ok(self.actual));
            }
            Stage::Idle => return None,
        };
        if let Err(error) = next {
            self.failure_stage = self.stage;
            self.stage = Stage::Idle;
            Some(Err(error))
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec::Vec;

    use libusb::{DESCRIPTOR_DEVICE, TransferProgress};

    use super::*;
    use crate::io::usb::hcd::{HcdSnapshot, RootPortState};

    struct FakeHcd {
        next: u32,
        submissions: Vec<(Direction, DataPid, usize)>,
    }

    impl HostController for FakeHcd {
        fn root_port_state(&self) -> RootPortState {
            RootPortState::Enabled(UsbSpeed::Full)
        }
        fn reset_root_port(&mut self, _deadline_us: u64) -> Result<(), UsbError> {
            Ok(())
        }
        fn submit(&mut self, request: TransferRequest<'_>) -> Result<TransferToken, UsbError> {
            self.next += 1;
            self.submissions
                .push((request.direction, request.pid, request.buffer.len()));
            Ok(TransferToken::new(0, self.next))
        }
        fn cancel(&mut self, _token: TransferToken) -> Result<TransferProgress, UsbError> {
            Ok(TransferProgress::Known(0))
        }
        fn reap(&mut self) -> Option<TransferCompletion> {
            None
        }
        fn service_timeouts(&mut self, _now_us: u64) {}
        fn snapshot(&self) -> HcdSnapshot {
            HcdSnapshot::default()
        }
    }

    fn completion(generation: u32, actual: usize) -> TransferCompletion {
        TransferCompletion {
            token: TransferToken::new(0, generation),
            result: Ok(actual),
            progress: TransferProgress::Known(actual),
        }
    }

    fn failed_completion(generation: u32, error: UsbError) -> TransferCompletion {
        TransferCompletion {
            token: TransferToken::new(0, generation),
            result: Err(error),
            progress: TransferProgress::Unknown,
        }
    }

    #[test]
    fn control_in_runs_setup_data_status() {
        let mut hcd = FakeHcd {
            next: 0,
            submissions: Vec::new(),
        };
        let mut control = ControlTransfer::new();
        control
            .start(
                &mut hcd,
                UsbAddress::DEFAULT,
                UsbSpeed::Full,
                8,
                SetupPacket::get_descriptor(DESCRIPTOR_DEVICE, 0, 0, 18),
                0,
            )
            .unwrap();
        assert!(control.complete(&mut hcd, completion(1, 8), 1).is_none());
        assert!(control.complete(&mut hcd, completion(2, 18), 2).is_none());
        assert_eq!(
            control.complete(&mut hcd, completion(3, 0), 3),
            Some(Ok(18))
        );
        assert_eq!(
            hcd.submissions,
            vec![
                (Direction::Out, DataPid::Setup, 8),
                (Direction::In, DataPid::Data1, 18),
                (Direction::Out, DataPid::Data1, 0),
            ]
        );
    }

    #[test]
    fn split_control_in_is_submitted_one_packet_at_a_time() {
        let mut hcd = FakeHcd {
            next: 0,
            submissions: Vec::new(),
        };
        let mut control = ControlTransfer::new();
        control
            .start_routed(
                &mut hcd,
                UsbAddress::new(3).unwrap(),
                UsbRoute {
                    device_speed: UsbSpeed::Low,
                    translator: Some(super::super::hcd::SplitTarget {
                        hub_address: UsbAddress::new(1).unwrap(),
                        port_number: 5,
                        hub_speed: UsbSpeed::High,
                    }),
                },
                8,
                SetupPacket::get_descriptor(DESCRIPTOR_DEVICE, 0, 0, 18),
                0,
            )
            .unwrap();
        assert!(control.complete(&mut hcd, completion(1, 8), 1).is_none());
        assert!(control.complete(&mut hcd, completion(2, 8), 2).is_none());
        assert!(control.complete(&mut hcd, completion(3, 8), 3).is_none());
        assert!(control.complete(&mut hcd, completion(4, 2), 4).is_none());
        assert_eq!(
            control.complete(&mut hcd, completion(5, 0), 5),
            Some(Ok(18))
        );
        assert_eq!(
            hcd.submissions,
            vec![
                (Direction::Out, DataPid::Setup, 8),
                (Direction::In, DataPid::Data1, 8),
                (Direction::In, DataPid::Data0, 8),
                (Direction::In, DataPid::Data1, 2),
                (Direction::Out, DataPid::Data1, 0),
            ]
        );
    }

    #[test]
    fn each_transfer_starts_its_data_stage_with_data1() {
        let mut hcd = FakeHcd {
            next: 0,
            submissions: Vec::new(),
        };
        let mut control = ControlTransfer::new();
        let route = UsbRoute {
            device_speed: UsbSpeed::Low,
            translator: Some(super::super::hcd::SplitTarget {
                hub_address: UsbAddress::new(1).unwrap(),
                port_number: 5,
                hub_speed: UsbSpeed::High,
            }),
        };
        // A 9-byte read leaves the transfer-local toggle on DATA0.
        control
            .start_routed(
                &mut hcd,
                UsbAddress::new(3).unwrap(),
                route,
                8,
                SetupPacket::get_descriptor(libusb::DESCRIPTOR_CONFIGURATION, 0, 0, 9),
                0,
            )
            .unwrap();
        assert!(control.complete(&mut hcd, completion(1, 8), 1).is_none());
        assert!(control.complete(&mut hcd, completion(2, 8), 2).is_none());
        assert!(control.complete(&mut hcd, completion(3, 1), 3).is_none());
        assert_eq!(control.complete(&mut hcd, completion(4, 0), 4), Some(Ok(9)));
        hcd.submissions.clear();
        control
            .start_routed(
                &mut hcd,
                UsbAddress::new(3).unwrap(),
                route,
                8,
                SetupPacket::get_descriptor(libusb::DESCRIPTOR_CONFIGURATION, 0, 0, 59),
                5,
            )
            .unwrap();
        assert!(control.complete(&mut hcd, completion(5, 8), 5).is_none());
        assert_eq!(
            hcd.submissions,
            vec![
                (Direction::Out, DataPid::Setup, 8),
                (Direction::In, DataPid::Data1, 8),
            ]
        );
    }

    #[test]
    fn transient_control_in_failure_restarts_from_setup() {
        let mut hcd = FakeHcd {
            next: 0,
            submissions: Vec::new(),
        };
        let mut control = ControlTransfer::new();
        control
            .start(
                &mut hcd,
                UsbAddress::new(3).unwrap(),
                UsbSpeed::Low,
                8,
                SetupPacket::get_descriptor(DESCRIPTOR_DEVICE, 0, 0, 18),
                0,
            )
            .unwrap();
        assert!(control.complete(&mut hcd, completion(1, 8), 1).is_none());
        assert!(
            control
                .complete(&mut hcd, failed_completion(2, UsbError::ControllerFault), 2)
                .is_none()
        );
        assert_eq!(
            hcd.submissions,
            vec![
                (Direction::Out, DataPid::Setup, 8),
                (Direction::In, DataPid::Data1, 18),
                (Direction::Out, DataPid::Setup, 8),
            ]
        );
    }

    #[test]
    fn split_transaction_failure_is_returned_for_tt_recovery() {
        let mut hcd = FakeHcd {
            next: 0,
            submissions: Vec::new(),
        };
        let mut control = ControlTransfer::new();
        control
            .start_routed(
                &mut hcd,
                UsbAddress::new(3).unwrap(),
                UsbRoute {
                    device_speed: UsbSpeed::Low,
                    translator: Some(super::super::hcd::SplitTarget {
                        hub_address: UsbAddress::new(1).unwrap(),
                        port_number: 5,
                        hub_speed: UsbSpeed::High,
                    }),
                },
                8,
                SetupPacket::get_descriptor(DESCRIPTOR_DEVICE, 0, 0, 18),
                0,
            )
            .unwrap();
        assert!(control.complete(&mut hcd, completion(1, 8), 1).is_none());
        assert_eq!(
            control.complete(&mut hcd, failed_completion(2, UsbError::Transaction), 2),
            Some(Err(UsbError::Transaction))
        );
        assert_eq!(hcd.submissions.len(), 2);
    }

    #[test]
    fn unknown_out_progress_is_not_retried() {
        let mut hcd = FakeHcd {
            next: 0,
            submissions: Vec::new(),
        };
        let mut control = ControlTransfer::new();
        control
            .start(
                &mut hcd,
                UsbAddress::new(1).unwrap(),
                UsbSpeed::Full,
                8,
                SetupPacket::new(0x00, 7, 0, 0, 8),
                0,
            )
            .unwrap();
        assert!(control.complete(&mut hcd, completion(1, 8), 1).is_none());
        assert_eq!(
            control.complete(&mut hcd, failed_completion(2, UsbError::Nyet), 2),
            Some(Err(UsbError::Nyet))
        );
        assert_eq!(hcd.submissions.len(), 2);
    }
}
