//! Efi Event Polling

use core::time::Duration;

use crate::*;

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
            uefi::boot::set_timer(&event, uefi::boot::TimerTrigger::Relative(duration)).unwrap();
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
