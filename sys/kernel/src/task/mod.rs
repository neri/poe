// Tasks
pub mod executor;
pub mod scheduler;
pub mod waker;

pub use waker::AtomicWaker;

use crate::arch::cpu::Cpu;
use alloc::boxed::Box;
use core::future::Future;
use core::pin::Pin;
use core::sync::atomic::*;
use core::task::{Context, Poll};

pub struct Task {
    id: TaskId,
    future: Pin<Box<dyn Future<Output = ()>>>,
}

impl Task {
    pub fn new(future: impl Future<Output = ()> + 'static) -> Task {
        Task {
            id: TaskId::new(),
            future: Box::pin(future),
        }
    }

    fn poll(&mut self, context: &mut Context) -> Poll<()> {
        self.future.as_mut().poll(context)
    }
}

#[derive(Debug, Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Default)]
struct TaskId(u64);

impl TaskId {
    fn new() -> Self {
        static NEXT_ID: AtomicU64 = AtomicU64::new(0);
        TaskId(Cpu::interlocked_increment_u64(&NEXT_ID))
    }
}
