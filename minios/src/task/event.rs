// use super::*;
use crate::{
    platform::{Platform, PlatformTrait},
    *,
};
use core::time::Duration;

// pub type EventCallback = *const fn(usize);

#[allow(unused)]
pub struct Event<'a> {
    state: EventState,
    poll: Option<Box<dyn PollingEvent + 'a>>,
}

impl<'a> Event<'a> {
    /// Create an event with the specified polling event.
    #[inline]
    pub fn with_polling(poll: impl PollingEvent + 'a) -> Self {
        let poll = Box::new(poll);
        let poll: Box<dyn PollingEvent + 'a> = poll;

        Self {
            state: EventState::Neutral,
            poll: Some(poll),
        }
    }

    /// Create an event that will be signaled after the specified duration.
    #[inline]
    pub fn with_timeout(duration: Duration) -> Self {
        Self::with_timer(TimerEvent::with_timeout(duration))
    }

    /// Create an event with the specified timer event.
    #[inline]
    pub fn with_timer(timer_event: TimerEvent) -> Self {
        let poller = TimerPoller { timer_event };
        Self::with_polling(poller)
    }
}

impl Event<'_> {
    #[inline]
    pub fn wait(&mut self) {
        System::wait_for_events(&mut [self]);
    }

    // pub fn signal(&mut self) {
    //     todo!()
    // }

    pub fn poll(&mut self) -> PollResult {
        if self.state == EventState::Signaled {
            self.state = EventState::Neutral;
            return PollResult::Ready;
        }
        if let Some(poll) = &mut self.poll {
            self.state = EventState::Polling;
            let result = poll.poll();
            self.state = EventState::Neutral;
            return result;
        }
        PollResult::Pending
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventState {
    Neutral,
    Polling,
    Signaled,
}

pub trait PollingEvent {
    fn poll(&mut self) -> PollResult;
}

pub enum PollResult {
    Ready,
    Pending,
}

#[allow(unused)]
struct TimerPoller {
    timer_event: TimerEvent,
}

impl PollingEvent for TimerPoller {
    fn poll(&mut self) -> PollResult {
        match self.timer_event {
            TimerEvent::Timeout(deadline) => {
                let result = Platform::monotonic().wrapping_sub(deadline) as i64;
                if result >= 0 {
                    PollResult::Ready
                } else {
                    PollResult::Pending
                }
            }
            TimerEvent::Periodic(interval) => {
                let now = Platform::monotonic();
                if now >= interval {
                    self.timer_event = TimerEvent::Periodic(now + interval);
                    PollResult::Ready
                } else {
                    PollResult::Pending
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimerEvent {
    Timeout(u64),
    Periodic(u64),
}

impl TimerEvent {
    /// Create a timer event that will be signaled as soon as possible.
    #[inline]
    pub fn epsilon() -> Self {
        Self::Timeout(1)
    }

    /// Create a timer event that will be signaled after the specified duration.
    #[inline]
    pub fn with_timeout(duration: Duration) -> Self {
        let ticks = Platform::duration_to_ticks(duration);
        Self::Timeout(Platform::monotonic() + ticks)
    }
}

/// Null event that is always ready.
pub struct NullEvent;

impl PollingEvent for NullEvent {
    fn poll(&mut self) -> PollResult {
        PollResult::Ready
    }
}
