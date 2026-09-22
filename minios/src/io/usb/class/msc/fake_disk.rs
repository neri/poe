//! A fake BOT disk for the tests: it answers the way section 6.7 of the BOT
//! specification says a device does, and takes injected faults.

use alloc::collections::VecDeque;
use alloc::vec;
use alloc::vec::Vec;

use libusb::UsbError;

use super::scsi::{self, sense_key};
use super::wire::{self, Csw, CswStatus};

pub(crate) fn pattern(block_size: u32, lba: u64, offset: usize) -> u8 {
    (lba.wrapping_mul(131) as u8)
        ^ (offset as u8)
        ^ ((offset >> 8) as u8).wrapping_mul(17)
        ^ (block_size >> 9) as u8
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Fault {
    StallCbw,
    ShortCbw,
    StallData,
    StallCsw,
    StallCswTwice,
    BadTag,
    BadSignature,
    PhaseError,
    /// The next bulk transfer never completes.
    Timeout,
    /// READ passes with one block fewer than asked for.
    ShortRead,
    /// READ passes with the right data and a non-zero residue.
    Residue,
    /// READ fails with this sense.
    ReadSense(u8, u8, u8),
    /// Mass Storage Reset stalls.
    ResetStalls,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DiskPhase {
    Command,
    Data,
    Status,
}

pub(crate) struct FakeDisk {
    pub(crate) block_size: u32,
    pub(crate) blocks: u64,
    pub(crate) max_lun: Option<u8>,
    pub(crate) device_type: u8,
    /// TEST UNIT READY fails with "becoming ready" this many more times.
    pub(crate) becoming_ready: u32,
    pub(crate) medium: bool,
    pub(crate) unit_attention: bool,
    /// Report 0xffffffff from READ CAPACITY(10).
    pub(crate) large: bool,
    pub(crate) rc16: bool,
    pub(crate) faults: VecDeque<Fault>,
    pub(crate) phase: DiskPhase,
    pub(crate) halted_in: bool,
    pub(crate) halted_out: bool,
    pub(crate) tag: u32,
    pub(crate) pending: Vec<u8>,
    pub(crate) sent: usize,
    pub(crate) expected: u32,
    pub(crate) status: CswStatus,
    pub(crate) residue: u32,
    pub(crate) residue_override: bool,
    pub(crate) sense: (u8, u8, u8),
    /// Opcodes of the commands received, in order.
    pub(crate) opcodes: Vec<u8>,
    pub(crate) controls: Vec<u8>,
    pub(crate) bulk_transfers: u64,
    /// OUT packets received towards the next CBW.
    pub(crate) out_staged: Vec<u8>,
    csw_left: Option<Vec<u8>>,
}

impl FakeDisk {
    pub(crate) fn new(block_size: u32, blocks: u64) -> Self {
        Self {
            block_size,
            blocks,
            max_lun: None,
            device_type: 0,
            becoming_ready: 0,
            medium: true,
            unit_attention: false,
            large: false,
            rc16: true,
            faults: VecDeque::new(),
            phase: DiskPhase::Command,
            halted_in: false,
            halted_out: false,
            tag: 0,
            pending: Vec::new(),
            sent: 0,
            expected: 0,
            status: CswStatus::Passed,
            residue: 0,
            residue_override: false,
            sense: (0, 0, 0),
            opcodes: Vec::new(),
            controls: Vec::new(),
            bulk_transfers: 0,
            out_staged: Vec::new(),
            csw_left: None,
        }
    }

    /// Takes one OUT packet of a CBW sent a packet at a time.
    pub(crate) fn stage_out(&mut self, data: &[u8]) {
        self.out_staged.extend_from_slice(data);
    }

    /// Acts on the CBW once all of it has arrived.
    pub(crate) fn flush_out(&mut self) -> Result<(), UsbError> {
        if self.out_staged.len() < wire::CBW_LEN {
            return Ok(());
        }
        let cbw = core::mem::take(&mut self.out_staged);
        self.bulk_out(&cbw).map(|_| ())
    }

    fn fault(&mut self, fault: Fault) -> bool {
        if self.faults.front() == Some(&fault) {
            self.faults.pop_front();
            true
        } else {
            false
        }
    }

    fn fail(&mut self, key: u8, asc: u8, ascq: u8) {
        self.status = CswStatus::Failed;
        self.sense = (key, asc, ascq);
        self.pending.clear();
    }

    fn execute(&mut self, cdb: &[u8]) {
        self.status = CswStatus::Passed;
        self.pending.clear();
        let opcode = cdb[0];
        self.opcodes.push(opcode);
        if opcode != scsi::opcode::REQUEST_SENSE && opcode != scsi::opcode::INQUIRY {
            if self.unit_attention {
                self.unit_attention = false;
                return self.fail(sense_key::UNIT_ATTENTION, 0x28, 0);
            }
            if !self.medium {
                return self.fail(sense_key::NOT_READY, scsi::ASC_MEDIUM_NOT_PRESENT, 0);
            }
            if self.becoming_ready > 0 {
                self.becoming_ready -= 1;
                return self.fail(sense_key::NOT_READY, scsi::ASC_NOT_READY, 1);
            }
        }
        let be32 = |b: &[u8]| u32::from_be_bytes([b[0], b[1], b[2], b[3]]);
        match opcode {
            scsi::opcode::TEST_UNIT_READY => {}
            scsi::opcode::REQUEST_SENSE => {
                let mut s = vec![0u8; 18];
                s[0] = 0x70;
                s[2] = self.sense.0;
                s[7] = 10;
                s[12] = self.sense.1;
                s[13] = self.sense.2;
                self.sense = (0, 0, 0);
                self.pending = s;
            }
            scsi::opcode::INQUIRY => {
                let mut b = vec![0u8; 36];
                b[0] = self.device_type;
                b[1] = 0x80;
                b[8..16].copy_from_slice(b"FAKE    ");
                b[16..32].copy_from_slice(b"BOT DISK        ");
                self.pending = b;
            }
            scsi::opcode::READ_CAPACITY_10 => {
                let last = if self.large {
                    0xffff_ffff
                } else {
                    (self.blocks - 1) as u32
                };
                let mut b = last.to_be_bytes().to_vec();
                b.extend_from_slice(&self.block_size.to_be_bytes());
                self.pending = b;
            }
            scsi::opcode::SERVICE_ACTION_IN_16 if self.rc16 => {
                let mut b = vec![0u8; 32];
                b[..8].copy_from_slice(&self.blocks.wrapping_sub(1).to_be_bytes());
                b[8..12].copy_from_slice(&self.block_size.to_be_bytes());
                self.pending = b;
            }
            scsi::opcode::READ_10 | scsi::opcode::READ_16 => {
                let (lba, count) = if opcode == scsi::opcode::READ_10 {
                    (
                        be32(&cdb[2..]) as u64,
                        u16::from_be_bytes([cdb[7], cdb[8]]) as u64,
                    )
                } else {
                    let mut l = [0u8; 8];
                    l.copy_from_slice(&cdb[2..10]);
                    (u64::from_be_bytes(l), be32(&cdb[10..]) as u64)
                };
                if lba + count > self.blocks {
                    return self.fail(sense_key::ILLEGAL_REQUEST, 0x21, 0);
                }
                if let Some(&Fault::ReadSense(k, a, q)) = self.faults.front() {
                    self.faults.pop_front();
                    return self.fail(k, a, q);
                }
                let mut data = Vec::new();
                for block in lba..lba + count {
                    for offset in 0..self.block_size as usize {
                        data.push(pattern(self.block_size, block, offset));
                    }
                }
                if self.fault(Fault::ShortRead) {
                    data.truncate(data.len() - self.block_size as usize);
                }
                if self.fault(Fault::Residue) {
                    self.residue_override = true;
                }
                self.pending = data;
            }
            _ => self.fail(sense_key::ILLEGAL_REQUEST, 0x20, 0),
        }
    }

    pub(crate) fn bulk_out(&mut self, data: &[u8]) -> Result<usize, UsbError> {
        self.bulk_transfers += 1;
        if self.fault(Fault::Timeout) {
            return Err(UsbError::Timeout);
        }
        if self.halted_out {
            return Err(UsbError::Stall);
        }
        if self.phase != DiskPhase::Command {
            // Section 6.6.1: a device expecting data or status does not take
            // a CBW.  Model it as never answering.
            return Err(UsbError::Timeout);
        }
        if self.fault(Fault::StallCbw) {
            self.halted_in = true;
            self.halted_out = true;
            return Err(UsbError::Stall);
        }
        if self.fault(Fault::ShortCbw) {
            return Ok(data.len() - 1);
        }
        assert_eq!(data.len(), wire::CBW_LEN);
        assert_eq!(&data[0..4], b"USBC");
        self.tag = u32::from_le_bytes([data[4], data[5], data[6], data[7]]);
        self.expected = u32::from_le_bytes([data[8], data[9], data[10], data[11]]);
        let len = data[14] as usize;
        let cdb = data[15..15 + len].to_vec();
        self.residue_override = false;
        self.execute(&cdb);
        self.sent = 0;
        let actual = self.pending.len().min(self.expected as usize);
        self.pending.truncate(actual);
        self.residue = self.expected - actual as u32;
        if self.residue_override {
            self.residue = 512;
        }
        self.phase = if self.expected > 0 {
            DiskPhase::Data
        } else {
            DiskPhase::Status
        };
        Ok(data.len())
    }

    pub(crate) fn bulk_in(&mut self, buffer: &mut [u8]) -> Result<usize, UsbError> {
        self.bulk_transfers += 1;
        if self.fault(Fault::Timeout) {
            return Err(UsbError::Timeout);
        }
        if self.halted_in {
            return Err(UsbError::Stall);
        }
        match self.phase {
            DiskPhase::Command => Err(UsbError::Timeout),
            DiskPhase::Data => {
                if self.fault(Fault::StallData) || self.sent == self.pending.len() {
                    // Section 6.7.2, the host expects more than the device
                    // has: it ends the data stage early with a STALL.
                    self.halted_in = true;
                    self.phase = DiskPhase::Status;
                    self.residue = self.expected - self.sent as u32;
                    return Err(UsbError::Stall);
                }
                let n = buffer.len().min(self.pending.len() - self.sent);
                buffer[..n].copy_from_slice(&self.pending[self.sent..self.sent + n]);
                self.sent += n;
                if self.sent == self.expected as usize {
                    self.phase = DiskPhase::Status;
                }
                Ok(n)
            }
            DiskPhase::Status => {
                if let Some(csw) = self.csw_left.as_mut() {
                    // The rest of a CSW longer than one packet.
                    let n = buffer.len().min(csw.len());
                    buffer[..n].copy_from_slice(&csw[..n]);
                    csw.drain(..n);
                    if csw.is_empty() {
                        self.csw_left = None;
                        self.phase = DiskPhase::Command;
                    }
                    return Ok(n);
                }
                if self.fault(Fault::StallCsw) {
                    self.halted_in = true;
                    return Err(UsbError::Stall);
                }
                if self.fault(Fault::StallCswTwice) {
                    self.halted_in = true;
                    self.faults.push_front(Fault::StallCsw);
                    return Err(UsbError::Stall);
                }
                let mut csw = Csw {
                    tag: self.tag,
                    residue: self.residue,
                    status: self.status,
                }
                .to_bytes();
                if self.fault(Fault::BadTag) {
                    csw[4] ^= 0xff;
                }
                if self.fault(Fault::BadSignature) {
                    csw[0] = 0;
                }
                if self.fault(Fault::PhaseError) {
                    csw[12] = 2;
                }
                let n = buffer.len().min(csw.len());
                buffer[..n].copy_from_slice(&csw[..n]);
                if n < csw.len() {
                    self.csw_left = Some(csw[n..].to_vec());
                } else {
                    self.phase = DiskPhase::Command;
                }
                Ok(n)
            }
        }
    }

    pub(crate) fn control(
        &mut self,
        setup: libusb::SetupPacket,
        buffer: &mut [u8],
    ) -> Result<usize, UsbError> {
        self.controls.push(setup.request);
        match (setup.request_type, setup.request) {
            (0xa1, 0xfe) => match self.max_lun {
                Some(lun) => {
                    buffer[0] = lun;
                    Ok(1)
                }
                None => Err(UsbError::Stall),
            },
            (0x21, 0xff) => {
                if self.fault(Fault::ResetStalls) {
                    return Err(UsbError::Stall);
                }
                self.phase = DiskPhase::Command;
                self.pending.clear();
                self.out_staged.clear();
                self.csw_left = None;
                Ok(0)
            }
            (0x02, 0x01) => {
                if setup.index & 0x80 != 0 {
                    self.halted_in = false;
                } else {
                    self.halted_out = false;
                }
                Ok(0)
            }
            _ => Err(UsbError::Stall),
        }
    }
}
