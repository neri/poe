use heapless::Deque;
use libhid::{Modifier, Usage};

use crate::io::hid_mgr::KeyStroke;

pub const BOOT_REPORT_SIZE: usize = 8;
pub const KEY_QUEUE_CAPACITY: usize = 32;

pub struct BootKeyboard {
    held: [u8; 6],
    queue: Deque<KeyStroke, KEY_QUEUE_CAPACITY>,
    dropped: u64,
    rollover: u64,
    delivered: u64,
}

impl BootKeyboard {
    pub const fn new() -> Self {
        Self {
            held: [0; 6],
            queue: Deque::new(),
            dropped: 0,
            rollover: 0,
            delivered: 0,
        }
    }
    pub fn consume_report(&mut self, report: &[u8]) -> Result<(), ()> {
        if report.len() != BOOT_REPORT_SIZE {
            return Err(());
        }
        if report[2..].iter().any(|key| matches!(*key, 1..=3)) {
            self.rollover = self.rollover.saturating_add(1);
            return Ok(());
        }
        let modifier = Modifier::from_bits_retain(report[0]);
        for key in report[2..].iter().copied().filter(|key| *key != 0) {
            if !self.held.contains(&key)
                && self
                    .queue
                    .push_back(KeyStroke {
                        usage: Usage(key),
                        modifier,
                    })
                    .is_err()
            {
                self.dropped = self.dropped.saturating_add(1)
            }
        }
        self.held.copy_from_slice(&report[2..]);
        Ok(())
    }
    pub fn pop(&mut self) -> Option<KeyStroke> {
        self.queue.pop_front()
    }
    pub const fn dropped(&self) -> u64 {
        self.dropped
    }
    pub const fn rollover_count(&self) -> u64 {
        self.rollover
    }
    /// Key presses handed to the console.
    pub const fn delivered(&self) -> u64 {
        self.delivered
    }

    pub fn drain_into_console(&mut self) {
        while let Some(key) = self.pop() {
            self.delivered = self.delivered.saturating_add(1);
            crate::io::usb::input::enqueue(key);
        }
    }
}

impl Default for BootKeyboard {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn emits_only_new_presses() {
        let mut k = BootKeyboard::new();
        let r = [2, 0, 4, 0, 0, 0, 0, 0];
        k.consume_report(&r).unwrap();
        assert_eq!(k.pop().unwrap().usage, Usage(4));
        k.consume_report(&r).unwrap();
        assert!(k.pop().is_none());
        k.consume_report(&[0; 8]).unwrap();
        k.consume_report(&r).unwrap();
        assert!(k.pop().is_some())
    }
    #[test]
    fn rollover_does_not_replace_held() {
        let mut k = BootKeyboard::new();
        k.consume_report(&[0, 0, 4, 0, 0, 0, 0, 0]).unwrap();
        k.pop();
        k.consume_report(&[0, 0, 1, 1, 1, 1, 1, 1]).unwrap();
        k.consume_report(&[0, 0, 4, 0, 0, 0, 0, 0]).unwrap();
        assert!(k.pop().is_none())
    }
}
