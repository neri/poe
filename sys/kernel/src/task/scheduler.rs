// Scheduler

use crate::arch::cpu::Cpu;
use alloc::boxed::Box;
use core::time::Duration;

static mut TIMER_SOURCE: Option<&'static dyn TimerSource> = None;

pub type TimeSpec = u64;

pub trait TimerSource {
    /// Create timer object from duration
    fn create(&self, duration: Duration) -> TimeSpec;

    /// Is that a timer before the deadline?
    fn until(&self, deadline: TimeSpec) -> bool;

    /// Get the value of the monotonic timer in microseconds
    fn monotonic(&self) -> Duration;
}

#[derive(Debug, Copy, Clone, Default)]
pub struct Timer {
    deadline: TimeSpec,
}

impl Timer {
    pub const JUST: Timer = Timer { deadline: 0 };

    #[inline]
    pub fn new(duration: Duration) -> Self {
        let timer = unsafe { TIMER_SOURCE.as_ref().unwrap() };
        Timer {
            deadline: timer.create(duration),
        }
    }

    #[inline]
    pub const fn is_just(&self) -> bool {
        self.deadline == 0
    }

    #[inline]
    pub fn until(&self) -> bool {
        if self.is_just() {
            false
        } else {
            let timer = unsafe { TIMER_SOURCE.as_ref().unwrap() };
            timer.until(self.deadline)
        }
    }

    #[inline]
    pub(crate) unsafe fn set_timer(source: &'static dyn TimerSource) {
        TIMER_SOURCE = Some(source);
    }

    #[track_caller]
    pub fn sleep(duration: Duration) {
        // TODO:
        let deadline = Timer::new(duration);
        while deadline.until() {
            unsafe {
                Cpu::halt();
            }
        }
    }

    #[inline]
    pub fn usleep(us: u64) {
        Self::sleep(Duration::from_micros(us));
    }

    #[inline]
    pub fn msleep(ms: u64) {
        Self::sleep(Duration::from_millis(ms));
    }

    #[inline]
    pub fn monotonic() -> Duration {
        unsafe { TIMER_SOURCE.as_ref() }.unwrap().monotonic()
    }

    #[inline]
    pub fn measure() -> u64 {
        Self::monotonic().as_micros() as u64
    }
}
