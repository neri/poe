//! PS2 Driver

use super::Irq;
use crate::io::hid_mgr::KeyStroke;
use crate::*;
use bitflags::bitflags;
use core::cell::UnsafeCell;
use libhid::*;
use x86::isolated_io::{LoIoPortRB, LoIoPortWB};

static mut PS2: UnsafeCell<Ps2> = UnsafeCell::new(Ps2::new());

pub struct Ps2 {
    key_phase: Ps2KeyPhase,
    key_modifier: Modifier,
    key_buffer: heapless::Vec<KeyStroke, 16>,
}

impl Ps2 {
    const fn new() -> Self {
        Self {
            key_phase: Ps2KeyPhase::Default,
            key_modifier: Modifier::empty(),
            key_buffer: heapless::Vec::new(),
        }
    }

    #[inline]
    unsafe fn shared_mut<'a>() -> &'a mut Self {
        unsafe { (&mut *(&raw mut PS2)).get_mut() }
    }

    pub unsafe fn init() -> Result<(), Ps2Error> {
        unsafe {
            let shared = Self::shared_mut();
            // shared.key_phase = Ps2KeyPhase::WaitAck;

            // // NO PS/2 Controller
            // match Self::wait_for_write(10) {
            //     Err(_) => return Err(Ps2Error::Unsupported),
            //     Ok(_) => (),
            // }

            // Self::write_cmd(Ps2Cmd::DISABLE_FIRST_PORT);
            // Self::send_cmd(Ps2Cmd::DISABLE_SECOND_PORT, 1)?;

            // while Self::read_status().contains(Ps2Status::OUTPUT_FULL) {
            //     let _ = Self::read_data();
            // }

            Irq(1).register(Self::irq_01).unwrap();
            // Irq(12).register(Self::irq_12).unwrap();

            // Self::send_cmd(Ps2Cmd::WRITE_CONFIG, 1)?;
            // Self::send_data(Ps2Data(0x44), 1)?;

            // // TODO: configure

            // Self::send_cmd(Ps2Cmd::ENABLE_FIRST_PORT, 1)?;
            // Self::send_cmd(Ps2Cmd::ENABLE_SECOND_PORT, 1)?;

            // Self::send_cmd(Ps2Cmd::WRITE_CONFIG, 1)?;
            // Self::send_data(Ps2Data(0x47), 1)?;

            // Self::send_data(Ps2Data::RESET_COMMAND, 1)?;
            // Timer::usleep(100_000);
            // Self::send_data(Ps2Data::ENABLE_SEND, 1)?;

            // Self::send_mouse_data(Ps2Data::RESET_COMMAND, 1)?;
            // Self::wait_mouse(10)?;
            // Self::send_mouse_data(Ps2Data::ENABLE_SEND, 1)?;

            System::set_stdin(shared);

            Ok(())
        }
    }

    /// IRQ1 Standard Keyboard
    fn irq_01(_irq: Irq) {
        let ps2 = unsafe { Self::shared_mut() };
        let data = unsafe { Self::read_data() };
        ps2.process_key_data(data);
    }

    #[inline]
    fn process_key_data(&mut self, data: Ps2Data) {
        if matches!(self.key_phase, Ps2KeyPhase::WaitAck) {
            self.key_phase = Ps2KeyPhase::Default;
            return;
        }
        if data == Ps2Data::PREFIX_E0 {
            self.key_phase = Ps2KeyPhase::PrefixE0;
        } else {
            let is_break = data.is_break();
            let mut scancode = data.scancode();

            match self.key_phase {
                Ps2KeyPhase::PrefixE0 => {
                    scancode |= 0x80;
                    self.key_phase = Ps2KeyPhase::Default;
                }
                _ => (),
            }
            let usage = Usage(PS2_TO_HID[scancode as usize]);
            if usage >= Usage::MOD_MIN && usage < Usage::MOD_MAX {
                let bit_position = Modifier::from_bits_retain(1 << (usage.0 - Usage::MOD_MIN.0));
                self.key_modifier.set(bit_position, !is_break);
            } else if !is_break {
                let _ = self
                    .key_buffer
                    .push(KeyStroke::new(usage, self.key_modifier));
            }
        }
    }

    #[allow(unused)]
    #[inline]
    unsafe fn read_data() -> Ps2Data {
        Ps2Data(unsafe { LoIoPortRB::<0x60>::new().read() })
    }

    #[allow(unused)]
    #[inline]
    unsafe fn write_data(data: Ps2Data) {
        unsafe {
            LoIoPortWB::<0x60>::new().write(data.0);
        }
    }

    #[allow(unused)]
    #[inline]
    unsafe fn read_status() -> Ps2Status {
        Ps2Status::from_bits_retain(unsafe { LoIoPortRB::<0x64>::new().read() })
    }

    #[allow(unused)]
    #[inline]
    unsafe fn write_cmd(command: Ps2Cmd) {
        unsafe {
            LoIoPortWB::<0x64>::new().write(command.0);
        }
    }

    #[allow(unused)]
    unsafe fn wait_for_write(timeout: u64) -> Result<(), Ps2Error> {
        for _ in 0..timeout {
            if unsafe { Self::read_status() }.contains(Ps2Status::INPUT_FULL) {
                Hal::cpu().no_op();
            } else {
                return Ok(());
            }
        }
        Err(Ps2Error::Timeout)
    }

    #[allow(unused)]
    unsafe fn wait_for_read(timeout: u64) -> Result<(), Ps2Error> {
        for _ in 0..timeout {
            if unsafe { Self::read_status() }.contains(Ps2Status::OUTPUT_FULL) {
                Hal::cpu().no_op();
            } else {
                return Ok(());
            }
        }
        Err(Ps2Error::Timeout)
    }

    // Wait for write, then command
    #[allow(unused)]
    unsafe fn send_cmd(command: Ps2Cmd, timeout: u64) -> Result<(), Ps2Error> {
        unsafe { Self::wait_for_write(timeout).map(|_| Self::write_cmd(command)) }
    }

    // Wait for write, then data
    #[allow(unused)]
    unsafe fn send_data(data: Ps2Data, timeout: u64) -> Result<(), Ps2Error> {
        unsafe { Self::wait_for_write(timeout).map(|_| Self::write_data(data)) }
    }

    // // Send to second port (mouse)
    // unsafe fn send_mouse_data(data: Ps2Data, timeout: u64) -> Result<(), Ps2Error> {
    //     Self::write_cmd(Ps2Cmd::WRITE_SECOND_PORT);
    //     Self::send_data(data, timeout)
    // }
}

impl SimpleTextInput for Ps2 {
    fn reset(&mut self) {
        self.key_buffer.clear();
    }

    fn is_ready(&mut self) -> bool {
        !self.key_buffer.is_empty()
    }

    fn read_key_stroke(&mut self) -> Option<NonZeroInputKey> {
        self.is_ready()
            .then(|| self.key_buffer.remove(0))
            .and_then(|key_stroke| InputKey::from_key_stroke(key_stroke).into())
    }
}

#[allow(unused)]
#[derive(Debug)]
pub(super) enum Ps2Error {
    Unsupported,
    Timeout,
}

#[allow(unused)]
#[derive(Debug)]
enum Ps2KeyPhase {
    Default,
    PrefixE0,
    WaitAck,
}

impl Default for Ps2KeyPhase {
    fn default() -> Self {
        Self::Default
    }
}

#[repr(transparent)]
#[derive(Debug, Copy, Clone, PartialEq)]
struct Ps2Data(pub u8);

#[allow(dead_code)]
impl Ps2Data {
    const ACK: Ps2Data = Ps2Data(0xFA);
    const NAK: Ps2Data = Ps2Data(0xFE);
    const ECHO: Ps2Data = Ps2Data(0xEE);

    const RESET_COMMAND: Ps2Data = Ps2Data(0xFF);
    const ENABLE_SEND: Ps2Data = Ps2Data(0xF4);
    const DISABLE_SEND: Ps2Data = Ps2Data(0xF5);
    const SET_DEFAULT: Ps2Data = Ps2Data(0xF6);

    const PREFIX_E0: Ps2Data = Ps2Data(0xE0);

    const fn is_break(self) -> bool {
        (self.0 & 0x80) != 0
    }

    const fn scancode(self) -> u8 {
        self.0 & 0x7F
    }
}

#[repr(transparent)]
#[derive(Debug, Copy, Clone, PartialEq)]
struct Ps2Cmd(pub u8);

#[allow(dead_code)]
impl Ps2Cmd {
    const WRITE_CONFIG: Ps2Cmd = Ps2Cmd(0x60);
    const DISABLE_SECOND_PORT: Ps2Cmd = Ps2Cmd(0xA7);
    const ENABLE_SECOND_PORT: Ps2Cmd = Ps2Cmd(0xA8);
    const DISABLE_FIRST_PORT: Ps2Cmd = Ps2Cmd(0xAD);
    const ENABLE_FIRST_PORT: Ps2Cmd = Ps2Cmd(0xAE);
    const WRITE_SECOND_PORT: Ps2Cmd = Ps2Cmd(0xD4);
}

bitflags! {
    struct Ps2Status: u8 {
        const OUTPUT_FULL = 0b0000_0001;
        const INPUT_FULL = 0b0000_0010;
        const SYSTEM_FLAG = 0b0000_0100;
        const COMMAND = 0b0000_1000;
        const TIMEOUT_ERROR = 0b0100_0000;
        const PARITY_ERROR = 0b1000_0000;
    }
}

/// PS2 scan code to HID usage table
#[rustfmt::skip]
static PS2_TO_HID: [u8; 256] = [
    /*            -0    -1    -2    -3    -4    -5    -6    -7    -8    -9    -A    -B    -C    -D    -E    -F */
    /*    0- */ 0x00, 0x29, 0x1e, 0x1f, 0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x2d, 0x2e, 0x2a, 0x2b,
    /*    1- */ 0x14, 0x1a, 0x08, 0x15, 0x17, 0x1c, 0x18, 0x0c, 0x12, 0x13, 0x2f, 0x30, 0x28, 0xe0, 0x04, 0x16,
    /*    2- */ 0x07, 0x09, 0x0a, 0x0b, 0x0d, 0x0e, 0x0f, 0x33, 0x34, 0x35, 0xe1, 0x31, 0x1d, 0x1b, 0x06, 0x19,
    /*    3- */ 0x05, 0x11, 0x10, 0x36, 0x37, 0x38, 0xe5, 0x55, 0xe2, 0x2c, 0x39, 0x3a, 0x3b, 0x3c, 0x3d, 0x3e,
    /*    4- */ 0x3f, 0x40, 0x41, 0x42, 0x43, 0x53, 0x47, 0x5f, 0x60, 0x61, 0x56, 0x5c, 0x5d, 0x5e, 0x57, 0x59,
    /*    5- */ 0x5a, 0x5b, 0x62, 0x63, 0x00, 0x00, 0x31, 0x44, 0x45, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    /*    6- */ 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    /*    7- */ 0x88, 0x00, 0x00, 0x87, 0x00, 0x00, 0x00, 0x00, 0x00, 0x8a, 0x00, 0x8b, 0x00, 0x89, 0x00, 0x00,
    /* e0 0- */ 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    /* e0 1- */ 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x58, 0xe4, 0x00, 0x00,
    /* e0 2- */ 0x7f, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x81, 0x00,
    /* e0 3- */ 0x80, 0x00, 0x00, 0x00, 0x00, 0x54, 0x00, 0x00, 0xe6, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    /* e0 4- */ 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x4a, 0x52, 0x4b, 0x00, 0x50, 0x00, 0x4f, 0x00, 0x4d,
    /* e0 5- */ 0x51, 0x4e, 0x49, 0x4c, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xe3, 0xe7, 0x65, 0x66, 0x00,
    /* e0 6- */ 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    /* e0 7- */ 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];
