//! Efi Event Polling

use crate::*;
use core::time::Duration;

pub struct EfiEventPoller {
    inner: uefi::Event,
}

impl EfiEventPoller {
    pub fn create_timer(duration: Duration) -> Self {
        unsafe {
            let event = uefi::boot::create_event(
                uefi::boot::EventType::TIMER,
                uefi::boot::Tpl::APPLICATION,
                None,
                None,
            )
            .unwrap();

            let nanos = duration.as_nanos();
            let timeout = if nanos < u64::MAX as u128 {
                nanos as u64 / 100
            } else {
                (nanos as f64 / 100.0) as u64
            };
            uefi::boot::set_timer(&event, uefi::boot::TimerTrigger::Relative(timeout)).unwrap();

            Self { inner: event }
        }
    }
}

impl PollingEvent for EfiEventPoller {
    fn poll(&mut self) -> PollResult {
        if uefi::boot::check_event(&self.inner).unwrap_or_default() {
            PollResult::Ready
        } else {
            PollResult::Pending
        }
    }
}
