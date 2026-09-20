use core::ptr::NonNull;

use heapless::Deque;

use crate::io::hid_mgr::KeyStroke;
use crate::io::tty::{InputKey, NonZeroInputKey, SimpleTextInput};

pub const QUEUE_CAPACITY: usize = 32;

struct Queue {
    keys: Deque<KeyStroke, QUEUE_CAPACITY>,
    dropped: u64,
}

static mut QUEUE: Queue = Queue {
    keys: Deque::new(),
    dropped: 0,
};

fn with_queue<T>(f: impl FnOnce(&mut Queue) -> T) -> T {
    // USB completion and console input are both serviced in foreground on the
    // boot CPU.  An AArch64 SpinMutex uses exclusive accesses, which are not
    // valid while early RAM still has Device memory attributes.
    unsafe { f(&mut *(&raw mut QUEUE)) }
}

pub fn enqueue(key: KeyStroke) {
    with_queue(|queue| {
        if queue.keys.push_back(key).is_err() {
            queue.dropped = queue.dropped.saturating_add(1);
        }
    })
}

/// Test-only drain of the console queue.
#[cfg(test)]
pub fn take() -> Option<KeyStroke> {
    with_queue(|queue| queue.keys.pop_front())
}

pub fn dropped() -> u64 {
    with_queue(|queue| queue.dropped)
}

/// Multiplexes USB key presses with the pre-existing UART input. USB
/// disconnect never changes or removes the fallback input.
pub struct UsbTextInputMux {
    fallback: NonNull<dyn SimpleTextInput>,
}

impl UsbTextInputMux {
    pub fn new(fallback: &'static mut dyn SimpleTextInput) -> Self {
        Self {
            fallback: NonNull::from(fallback),
        }
    }

    fn usb_key() -> Option<NonZeroInputKey> {
        let key = with_queue(|queue| queue.keys.pop_front())?;
        InputKey::from_key_stroke(key).into()
    }
}

impl SimpleTextInput for UsbTextInputMux {
    fn reset(&mut self) {
        with_queue(|queue| queue.keys.clear());
        unsafe { self.fallback.as_mut() }.reset();
    }

    fn read_key_stroke(&mut self) -> Option<NonZeroInputKey> {
        Self::usb_key().or_else(|| unsafe { self.fallback.as_mut() }.read_key_stroke())
    }

    fn is_ready(&mut self) -> bool {
        with_queue(|queue| !queue.keys.is_empty()) || unsafe { self.fallback.as_mut() }.is_ready()
    }
}
