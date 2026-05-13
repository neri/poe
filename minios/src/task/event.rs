//! Event subsystem

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
    pub fn polling(poll: impl PollingEvent + 'a) -> Self {
        let poll = Box::new(poll);
        let poll: Box<dyn PollingEvent + 'a> = poll;

        Self {
            state: EventState::Idle,
            poll: Some(poll),
        }
    }

    /// Create an event that will be signaled after the specified duration.
    #[inline]
    pub fn with_timeout(duration: Duration) -> Self {
        Self {
            state: EventState::Idle,
            poll: Some(Platform::create_timer_event(duration)),
        }
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
            self.state = EventState::Idle;
            return PollResult::Ready;
        }
        if let Some(poll) = &mut self.poll {
            self.state = EventState::Polling;
            let result = poll.poll();
            self.state = EventState::Idle;
            return result;
        }
        PollResult::Pending
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventState {
    Idle,
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

/// Null event that is always ready.
pub struct NullEvent;

impl PollingEvent for NullEvent {
    fn poll(&mut self) -> PollResult {
        PollResult::Ready
    }
}
