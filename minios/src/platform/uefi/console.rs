//! Console implementation using UEFI

use crate::io::hid_mgr::HidManager;
use crate::*;
use core::cell::UnsafeCell;
use core::mem::transmute;
use libhid::layouts::KeyStroke;
use uefi::proto::console::text::Key;

pub struct UefiConsole {
    last_input: Option<NonZeroInputKey>,
}

static mut SHARED: UnsafeCell<UefiConsole> = UnsafeCell::new(UefiConsole::new());

impl UefiConsole {
    #[inline]
    const fn new() -> Self {
        Self { last_input: None }
    }

    #[inline]
    pub unsafe fn init() {
        unsafe {
            let stdout = Self::shared();
            (stdout as &mut dyn SimpleTextOutput).reset();
            System::set_stdout(stdout);

            // Safety:
            let stdin = Self::shared();
            (stdin as &mut dyn SimpleTextInput).reset();
            System::set_stdin(stdin);
        }
    }

    #[inline]
    const fn shared() -> &'static mut UefiConsole {
        unsafe { (&mut *(&raw mut SHARED)).get_mut() }
    }

    fn refill(&mut self) {
        if self.last_input.is_some() {
            return;
        }

        let Ok(key_event) = uefi::system::with_stdin(|v| v.wait_for_key_event()) else {
            return;
        };
        if !uefi::boot::check_event(&key_event).unwrap_or(false) {
            return;
        }
        let key = uefi::system::with_stdin(|v| v.read_key())
            .ok()
            .flatten()
            .unwrap();

        match key {
            Key::Printable(unicode) => {
                let unicode = unicode.into();
                let Some(ks) = HidManager::estimate_key_stroke_from_char(unicode) else {
                    return;
                };
                let input_key = InputKey::new(ks, unicode as u16);
                self.last_input = NonZeroInputKey::from_input_key(input_key);
            }
            Key::Special(scan_code) => {
                use uefi::proto::console::text::ScanCode;
                let usage = match scan_code {
                    ScanCode::ESCAPE => Some(Usage::KEY_ESCAPE),
                    ScanCode::DELETE => Some(Usage::KEY_DELETE),
                    ScanCode::UP => Some(Usage::KEY_UP_ARROW),
                    ScanCode::DOWN => Some(Usage::KEY_DOWN_ARROW),
                    ScanCode::LEFT => Some(Usage::KEY_LEFT_ARROW),
                    ScanCode::RIGHT => Some(Usage::KEY_RIGHT_ARROW),
                    _ => None,
                };
                self.last_input = usage
                    .map(KeyStroke::from_usage)
                    .and_then(|ks| NonZeroInputKey::from_input_key(InputKey::new(ks, 0)));
            }
        }
    }

    #[inline]
    pub(super) fn handover() {
        (Self::shared() as &mut dyn SimpleTextOutput).reset();
    }
}

impl core::fmt::Write for UefiConsole {
    #[inline]
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let _ = uefi::system::with_stdout(|v| v.write_str(s));
        Ok(())
    }
}

impl SimpleTextOutput for UefiConsole {
    #[inline]
    fn reset(&mut self) {
        let _ = uefi::system::with_stdout(|v| v.reset(true));
        self.set_attribute(0);
        self.clear_screen();
    }

    fn clear_screen(&mut self) {
        let _ = uefi::system::with_stdout(|v| v.clear());
    }

    fn set_attribute(&mut self, attribute: u8) {
        let bg = unsafe { transmute((attribute >> 4) & 7) };
        let fg = unsafe { transmute(attribute & 0x0f) };
        let _ = uefi::system::with_stdout(|v| v.set_color(fg, bg));
    }

    fn set_cursor_position(&mut self, col: u32, row: u32) {
        let _ = uefi::system::with_stdout(|v| v.set_cursor_position(col as usize, row as usize));
    }

    fn enable_cursor(&mut self, visible: bool) -> bool {
        uefi::system::with_stdout(|v| {
            let result = v.cursor_visible();
            let _ = v.enable_cursor(visible);
            result
        })
    }

    fn current_mode(&mut self) -> SimpleTextOutputMode {
        let mut result = SimpleTextOutputMode::new();
        let _ = uefi::system::with_stdout(|v| {
            let Ok(Some(mode)) = v.current_mode() else {
                return;
            };
            result.columns = mode.columns().clamp(0, 255) as u8;
            result.rows = mode.rows().clamp(0, 255) as u8;
            result.cursor_column = v.cursor_position().0.clamp(0, 255) as u8;
            result.cursor_row = v.cursor_position().1.clamp(0, 255) as u8;
            result.cursor_visible = v.cursor_visible().into();
            if result.columns >= 80 && result.rows >= 25 {
                result.rows -= 1;
            }
        });
        result
    }
}

impl SimpleTextInput for UefiConsole {
    fn reset(&mut self) {
        let _ = uefi::system::with_stdin(|v| v.reset(true));
    }

    fn read_key_stroke(&mut self) -> Option<NonZeroInputKey> {
        self.refill();
        self.last_input.take()
    }

    fn is_ready(&mut self) -> bool {
        self.refill();
        self.last_input.is_some()
    }
}
