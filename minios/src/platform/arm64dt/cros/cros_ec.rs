//! ChromeOS EC on SPI
//!
//! Follows depthcharge (drivers/ec/cros/spi.c and ec.c) of the gru firmware.
//! Every wait has a time limit, so that the boot continues even if the EC does not respond.
//! The SPI controller is abstracted by [`SpiDevice`].

use fdt::PropName;

use super::ec_packet::{self, HEADER_SIZE, ResponseError};
use crate::platform::arm64dt::spi::SpiDevice;
use crate::platform::arm64dt::{counter_us, delay_us, dt};

const EC_CMD_PWM_SET_DUTY: u16 = 0x0025;
const EC_CMD_MKBP_STATE: u16 = 0x0060;
const EC_CMD_GET_NEXT_EVENT: u16 = 0x0067;
const EC_PWM_TYPE_DISPLAY_LIGHT: u8 = 2;
const EC_PWM_MAX_DUTY: u32 = 65535;

const EC_SPI_FRAME_START: u8 = 0xec;
const EC_SPI_PROCESSING: u8 = 0xfa;
const EC_SPI_RX_BAD_DATA: u8 = 0xfb;
const EC_SPI_NOT_READY: u8 = 0xfc;

/// Time to keep CS deasserted between transactions
const CS_COOLDOWN_US: u64 = 200;
/// Time for the EC to wake up after asserting CS (bob)
const WAKEUP_DELAY_US: u64 = 100;
/// Time for the EC to accept a packet
const ACCEPT_TIMEOUT_US: u64 = 5_000;
/// Time for the EC to process a packet
const PROCESS_TIMEOUT_US: u64 = 1_000_000;

const EC_RES_INVALID_COMMAND: u16 = 1;
const EC_RES_UNAVAILABLE: u16 = 9;

const EC_MKBP_EVENT_KEY_MATRIX: u8 = 0;

/// Bytes of the keyboard matrix (one per column)
pub const KEY_MATRIX_SIZE: usize = 13;

const MAX_DATA: usize = 32;

/// Backlight brightness set by depthcharge when the screen is on (percent)
pub const DEFAULT_BACKLIGHT: u32 = 80;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// Timed out waiting for the SPI controller
    Spi,
    /// The EC did not accept or finish the command in time
    Timeout,
    /// The EC received bad data
    BadData,
    /// The EC was not ready
    NotReady,
    /// Bad request or response packet
    Packet(ResponseError),
}

impl Error {
    /// Returns whether the EC does not support the command.
    #[inline]
    pub fn is_invalid_command(&self) -> bool {
        *self == Self::Packet(ResponseError::Result(EC_RES_INVALID_COMMAND))
    }
}

/// Finds the EC on SPI in the device tree.
///
/// `spi_device` makes the SPI device from the controller node, its `reg` (CPU address, size)
/// and the chip select of the EC. It returns `None` if the controller is not supported.
pub fn find<S: SpiDevice>(
    dt: &fdt::DeviceTree,
    mut spi_device: impl FnMut(&fdt::Node, (usize, usize), u32) -> Option<S>,
) -> Option<CrosEc<S>> {
    dt::find_map(dt, |controller, map| {
        let ec = controller
            .children()
            .find(|v| v.status_is_ok() && v.is_compatible_with("google,cros-ec-spi"))?;
        let cs = ec.get_prop_u32(PropName::REG)?;
        let reg = map.reg(controller, 0)?;
        spi_device(controller, reg, cs).map(|spi| CrosEc { spi })
    })
}

pub struct CrosEc<S: SpiDevice> {
    spi: S,
}

impl<S: SpiDevice> CrosEc<S> {
    /// Sets the brightness of the display backlight, the same way as depthcharge.
    pub unsafe fn set_display_backlight(&self, percent: u32) -> Result<(), Error> {
        let duty = (percent.min(100) * EC_PWM_MAX_DUTY / 100) as u16;
        let duty = duty.to_le_bytes();
        let params = [duty[0], duty[1], EC_PWM_TYPE_DISPLAY_LIGHT, 0];
        unsafe { self.command(EC_CMD_PWM_SET_DUTY, 0, &params, &mut []) }.map(|_| ())
    }

    /// Gets the next MKBP event and returns the keyboard matrix if it is a key matrix event.
    ///
    /// Returns `Ok(None)` if no event is pending or the event is not a key matrix event.
    pub unsafe fn next_key_matrix_event(&self) -> Result<Option<[u8; KEY_MATRIX_SIZE]>, Error> {
        let mut event = [0u8; 1 + 16];
        match unsafe { self.command(EC_CMD_GET_NEXT_EVENT, 0, &[], &mut event) } {
            Ok(len) if len > KEY_MATRIX_SIZE && event[0] == EC_MKBP_EVENT_KEY_MATRIX => {
                let mut matrix = [0u8; KEY_MATRIX_SIZE];
                matrix.copy_from_slice(&event[1..1 + KEY_MATRIX_SIZE]);
                Ok(Some(matrix))
            }
            Ok(_) => Ok(None),
            Err(Error::Packet(ResponseError::Result(EC_RES_UNAVAILABLE))) => Ok(None),
            Err(err) => Err(err),
        }
    }

    /// Gets the current state of the keyboard matrix (for the EC without MKBP events).
    pub unsafe fn key_matrix_state(&self) -> Result<[u8; KEY_MATRIX_SIZE], Error> {
        let mut matrix = [0u8; KEY_MATRIX_SIZE];
        match unsafe { self.command(EC_CMD_MKBP_STATE, 0, &[], &mut matrix) }? {
            KEY_MATRIX_SIZE => Ok(matrix),
            _ => Err(Error::Packet(ResponseError::Invalid)),
        }
    }

    /// Sends a host command and returns the size of the response data.
    pub unsafe fn command(
        &self,
        command: u16,
        version: u8,
        data: &[u8],
        response_data: &mut [u8],
    ) -> Result<usize, Error> {
        let mut request = [0u8; HEADER_SIZE + MAX_DATA];
        let mut response = [0u8; HEADER_SIZE + MAX_DATA];
        if response_data.len() > MAX_DATA {
            return Err(Error::Packet(ResponseError::Invalid));
        }
        let request_size = ec_packet::build_request(&mut request, command, version, data)
            .ok_or(Error::Packet(ResponseError::Invalid))?;
        let response = &mut response[..HEADER_SIZE + response_data.len()];

        unsafe { self.send_packet(&request[..request_size], response)? };

        let range = ec_packet::parse_response(response).map_err(Error::Packet)?;
        let len = range.len();
        response_data[..len].copy_from_slice(&response[range]);
        Ok(len)
    }

    unsafe fn send_packet(&self, request: &[u8], response: &mut [u8]) -> Result<(), Error> {
        delay_us(CS_COOLDOWN_US);
        unsafe {
            self.spi.select();
            delay_us(WAKEUP_DELAY_US);
            let result = self.transact(request, response);
            self.spi.deselect();
            result
        }
    }

    unsafe fn transact(&self, request: &[u8], response: &mut [u8]) -> Result<(), Error> {
        unsafe {
            self.spi.write(request).map_err(|_| Error::Spi)?;
            self.wait_for_frame()?;
            self.spi.read(response).map_err(|_| Error::Spi)
        }
    }

    /// Waits for the start of the response frame.
    unsafe fn wait_for_frame(&self) -> Result<(), Error> {
        let start = counter_us();
        let mut accepted = false;
        loop {
            let mut byte = [0u8];
            unsafe { self.spi.read(&mut byte).map_err(|_| Error::Spi)? };
            match byte[0] {
                EC_SPI_FRAME_START => return Ok(()),
                EC_SPI_PROCESSING => accepted = true,
                EC_SPI_RX_BAD_DATA => return Err(Error::BadData),
                EC_SPI_NOT_READY => return Err(Error::NotReady),
                _ => {}
            }
            let waited = counter_us().wrapping_sub(start);
            if (!accepted && waited > ACCEPT_TIMEOUT_US) || waited > PROCESS_TIMEOUT_US {
                return Err(Error::Timeout);
            }
        }
    }
}
