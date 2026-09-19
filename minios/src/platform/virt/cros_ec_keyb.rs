//! Keyboard of Chromebooks, scanned by the ChromeOS EC
//!
//! The EC is polled (without its interrupt) when the input is read,
//! at most once per `POLL_INTERVAL_US`. Keys are not repeated.

use core::cell::UnsafeCell;

use super::counter_us;
use super::cros_ec::{CrosEc, Error, KEY_MATRIX_SIZE};
use super::keymatrix::KeyMatrix;
use super::vpd::KeyboardLayout;
use crate::io::hid_mgr::{HidManager, KeyStroke};
use crate::*;

static mut KEYBOARD: UnsafeCell<Option<CrosEcKeyboard>> = UnsafeCell::new(None);

pub struct CrosEcKeyboard {
    ec: CrosEc,
    mode: Mode,
    matrix: KeyMatrix,
    key_buffer: heapless::Vec<KeyStroke, 16>,
    next_poll: u64,
}

/// How to read the keyboard matrix
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// Key matrix events (EC_CMD_GET_NEXT_EVENT), as depthcharge on bob does
    NextEvent,
    /// Current state (EC_CMD_MKBP_STATE), for the EC without MKBP events
    State,
}

impl CrosEcKeyboard {
    const POLL_INTERVAL_US: u64 = 10_000;
    /// Interval after an EC error, not to slow down the system with timeouts
    const RETRY_INTERVAL_US: u64 = 1_000_000;
    /// Maximum number of events read at once
    const MAX_EVENTS: usize = 8;
    /// Maximum number of stale events discarded at the start
    const MAX_STALE_EVENTS: usize = 32;

    /// Uses the keyboard as stdin, with the layout.
    ///
    /// Returns `false` if the EC does not answer.
    pub unsafe fn install(ec: CrosEc, layout: KeyboardLayout) -> bool {
        // Discard the events queued so far (e.g. Ctrl+U at the firmware screen),
        // which also checks that the EC answers.
        let mut matrix = KeyMatrix::new();
        let mode = match unsafe { Self::discard_stale_events(&ec, &mut matrix) } {
            Ok(()) => Mode::NextEvent,
            Err(err) if err.is_invalid_command() => match unsafe { ec.key_matrix_state() } {
                Ok(state) => {
                    matrix.reset(&state);
                    Mode::State
                }
                Err(_) => return false,
            },
            Err(_) => return false,
        };

        unsafe {
            let shared = (&mut *(&raw mut KEYBOARD)).get_mut();
            *shared = Some(Self {
                ec,
                mode,
                matrix,
                key_buffer: heapless::Vec::new(),
                next_poll: 0,
            });
            match layout {
                KeyboardLayout::Japanese => HidManager::set_japanese_layout(),
                KeyboardLayout::Us => {}
            }
            System::set_stdin(shared.as_mut().unwrap());
        }
        true
    }

    unsafe fn discard_stale_events(ec: &CrosEc, matrix: &mut KeyMatrix) -> Result<(), Error> {
        for _ in 0..Self::MAX_STALE_EVENTS {
            match unsafe { ec.next_key_matrix_event() }? {
                Some(state) => matrix.reset(&state),
                None => break,
            }
        }
        Ok(())
    }

    fn poll(&mut self) {
        let now = counter_us();
        if now < self.next_poll {
            return;
        }
        let result = unsafe {
            match self.mode {
                Mode::NextEvent => self.poll_events(),
                Mode::State => self.poll_state(),
            }
        };
        self.next_poll = now
            + if result.is_ok() {
                Self::POLL_INTERVAL_US
            } else {
                Self::RETRY_INTERVAL_US
            };
    }

    unsafe fn poll_events(&mut self) -> Result<(), Error> {
        for _ in 0..Self::MAX_EVENTS {
            match unsafe { self.ec.next_key_matrix_event() }? {
                Some(state) => self.process(&state),
                None => break,
            }
        }
        Ok(())
    }

    unsafe fn poll_state(&mut self) -> Result<(), Error> {
        let state = unsafe { self.ec.key_matrix_state() }?;
        self.process(&state);
        Ok(())
    }

    fn process(&mut self, state: &[u8; KEY_MATRIX_SIZE]) {
        let key_buffer = &mut self.key_buffer;
        self.matrix.update(state, |key_stroke| {
            let _ = key_buffer.push(key_stroke);
        });
    }
}

impl SimpleTextInput for CrosEcKeyboard {
    fn reset(&mut self) {
        self.key_buffer.clear();
    }

    fn is_ready(&mut self) -> bool {
        self.poll();
        !self.key_buffer.is_empty()
    }

    fn read_key_stroke(&mut self) -> Option<NonZeroInputKey> {
        self.is_ready()
            .then(|| self.key_buffer.remove(0))
            .and_then(|key_stroke| InputKey::from_key_stroke(key_stroke).into())
    }
}
