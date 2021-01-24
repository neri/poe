// A Window System

use crate::graphics::bitmap::*;
use crate::graphics::color::*;
use crate::graphics::coords::*;
use crate::{io::hid::*, system::System};

static mut WM: WindowManager = WindowManager::new();

pub struct WindowManager {
    last_key: Option<char>,
    pointer_x: isize,
    pointer_y: isize,
}

impl WindowManager {
    const fn new() -> Self {
        Self {
            last_key: None,
            pointer_x: 320,
            pointer_y: 240,
        }
    }

    #[inline]
    fn shared<'a>() -> &'a mut Self {
        unsafe { &mut WM }
    }

    pub fn post_key_event(event: KeyEvent) {
        // TODO:
        // let shared = match Self::shared_opt() {
        //     Some(v) => v,
        //     None => return,
        // };
        let shared = Self::shared();
        if let Some(event) = event.key_data() {
            shared.last_key = Some(event.into_char());
        }
    }

    pub fn post_mouse_event(mouse_state: &mut MouseState) {
        // TODO:
        // let shared = match Self::shared_opt() {
        //     Some(v) => v,
        //     None => return,
        // };
        let shared = Self::shared();

        let bitmap = System::main_screen();

        let mut pointer = Point::new(0, 0);
        core::mem::swap(&mut mouse_state.x, &mut pointer.x);
        core::mem::swap(&mut mouse_state.y, &mut pointer.y);
        let button_changes: MouseButton = mouse_state.current_buttons ^ mouse_state.prev_buttons;
        let button_down: MouseButton = button_changes & mouse_state.current_buttons;
        let button_up: MouseButton = button_changes & mouse_state.prev_buttons;
        let button_changed = !button_changes.is_empty();

        let mut x = shared.pointer_x + pointer.x;
        let mut y = shared.pointer_y + pointer.y;
        if x < 0 {
            x = 0;
        } else if x >= bitmap.width() as isize {
            x = bitmap.width() as isize - 1;
        }
        if y < 0 {
            y = 0;
        } else if y >= bitmap.height() as isize {
            y = bitmap.height() as isize - 1;
        }
        shared.pointer_x = x;
        shared.pointer_y = y;

        let origin = Point::new(x, y);
        bitmap.fill_circle(origin, 4, IndexedColor::WHITE);
        if mouse_state.current_buttons.contains(MouseButton::LEFT) {
            bitmap.fill_circle(origin, 3, IndexedColor::RED);
        } else if mouse_state.current_buttons.contains(MouseButton::RIGHT) {
            bitmap.fill_circle(origin, 3, IndexedColor::BLUE);
        } else {
            bitmap.fill_circle(origin, 3, IndexedColor::BLACK);
        }
    }

    pub fn get_key() -> Option<char> {
        let shared = Self::shared();
        core::mem::replace(&mut shared.last_key, None)
    }
}
