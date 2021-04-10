// Scheduler

use super::executor::Executor;
use super::*;
use crate::{
    arch::cpu::{Cpu, CpuContextData},
    mem::string::StringBuffer,
    rt::Personality,
    sync::atomicflags::AtomicBitflags,
    sync::fifo::*,
    sync::semaphore::Semaphore,
    window::*,
    *,
};
use alloc::collections::btree_map::BTreeMap;
use alloc::sync::Arc;
use alloc::vec::*;
use bitflags::*;
use core::cell::UnsafeCell;
use core::ffi::c_void;
use core::fmt::Write;
use core::num::NonZeroUsize;
use core::ops::Add;
use core::sync::atomic::*;
use core::time::Duration;

static mut SCHEDULER: Option<Box<Scheduler>> = None;

static SCHEDULER_ENABLED: AtomicBool = AtomicBool::new(false);

pub struct Scheduler {
    queue_realtime: ThreadQueue,
    queue_higher: ThreadQueue,
    queue_normal: ThreadQueue,
    queue_lower: ThreadQueue,
    pool: ThreadPool,

    usage: AtomicUsize,

    timer_events: Vec<TimerEvent>,

    idle: ThreadHandle,
    current: ThreadHandle,
    retired: Option<ThreadHandle>,
}

impl Scheduler {
    const MAX_STATISTICS: usize = 1000;

    /// Start scheduler and sleep forever
    pub(crate) unsafe fn start(f: fn(usize) -> (), args: usize) -> ! {
        const SIZE_OF_SUB_QUEUE: usize = 64;
        const SIZE_OF_MAIN_QUEUE: usize = 256;

        let queue_realtime = ThreadQueue::with_capacity(SIZE_OF_SUB_QUEUE);
        let queue_higher = ThreadQueue::with_capacity(SIZE_OF_SUB_QUEUE);
        let queue_normal = ThreadQueue::with_capacity(SIZE_OF_MAIN_QUEUE);
        let queue_lower = ThreadQueue::with_capacity(SIZE_OF_SUB_QUEUE);

        let mut pool = ThreadPool::default();
        let idle = {
            let idle = RawThread::new(ProcessId(0), Priority::Idle, "Idle", None, 0, None);
            let handle = idle.handle;
            pool.add(Box::new(idle));
            handle
        };

        SCHEDULER = Some(Box::new(Self {
            pool,
            queue_realtime,
            queue_higher,
            queue_normal,
            queue_lower,
            timer_events: Vec::with_capacity(100),
            idle,
            current: idle,
            retired: None,
            usage: AtomicUsize::new(0),
        }));

        SpawnOption::with_priority(Priority::Normal).spawn(f, args, "System");

        SpawnOption::with_priority(Priority::Realtime).spawn(
            Self::statistics_thread,
            0,
            "Statistics",
        );

        SCHEDULER_ENABLED.store(true, Ordering::SeqCst);

        loop {
            Cpu::halt();
        }
    }

    #[inline]
    #[track_caller]
    fn shared<'a>() -> &'a mut Self {
        unsafe { SCHEDULER.as_mut().unwrap() }
    }

    #[inline]
    pub fn usage_per_cpu() -> usize {
        let shared = Self::shared();
        shared.usage.load(Ordering::Relaxed)
    }

    #[inline]
    pub fn usage_total() -> usize {
        Self::usage_per_cpu()
    }

    /// Measuring Statistics
    fn statistics_thread(_: usize) {
        let shared = Self::shared();

        let expect = 1_000_000;
        let interval = Duration::from_micros(expect as u64);
        let mut measure = Timer::measure().0;
        loop {
            Timer::sleep(interval);

            let now = Timer::measure().0;
            let actual = now - measure;
            let actual1000 = actual as usize * Self::MAX_STATISTICS;

            let mut usage = 0;
            for thread in shared.pool.data.values() {
                let thread = thread.clone();
                let thread = unsafe { &mut (*thread.get()) };
                let load0 = thread.load0.swap(0, Ordering::SeqCst);
                let load = usize::min(
                    load0 as usize * expect as usize / actual1000,
                    Self::MAX_STATISTICS,
                );
                thread.load.store(load as u32, Ordering::SeqCst);
                if thread.priority != Priority::Idle {
                    usage += load;
                }
            }

            shared
                .usage
                .store(usize::min(usage, Self::MAX_STATISTICS), Ordering::SeqCst);

            measure = now;
        }
    }

    pub fn print_statistics(sb: &mut StringBuffer, exclude_idle: bool) {
        let sch = Self::shared();
        writeln!(sb, "PID PRI %CPU TIME     NAME").unwrap();
        for thread in sch.pool.data.values() {
            let thread = thread.clone();
            let thread = unsafe { &(*thread.get()) };
            if exclude_idle && thread.priority == Priority::Idle {
                continue;
            }

            let load = u32::min(thread.load.load(Ordering::Relaxed), 999);
            let load0 = load % 10;
            let load1 = load / 10;
            write!(
                sb,
                "{:3} {} {} {:2}.{:1}",
                thread.pid.0, thread.priority as usize, thread.attribute, load1, load0,
            )
            .unwrap();

            let time = Duration::from(TimeSpec(thread.cpu_time.load(Ordering::Relaxed)));
            let secs = time.as_secs() as usize;
            let sec = secs % 60;
            let min = secs / 60 % 60;
            let hour = secs / 3600;
            if hour > 0 {
                write!(sb, " {:02}:{:02}:{:02}", hour, min, sec,).unwrap();
            } else {
                let dsec = time.subsec_nanos() / 10_000_000;
                write!(sb, " {:02}:{:02}.{:02}", min, sec, dsec,).unwrap();
            }

            match thread.name() {
                Some(name) => writeln!(sb, " {}", name,).unwrap(),
                None => writeln!(sb, " ({})", thread.handle.as_usize(),).unwrap(),
            }
        }
    }

    /// Get the current process if possible
    #[inline]
    pub fn current_pid() -> Option<ProcessId> {
        if Self::is_enabled() {
            Self::current_thread().map(|thread| thread.as_ref().pid)
        } else {
            None
        }
    }

    /// Get the current thread running on the current processor
    #[inline]
    pub fn current_thread() -> Option<ThreadHandle> {
        unsafe {
            Cpu::without_interrupts(|| {
                if Self::is_enabled() {
                    let shared = Self::shared();
                    Some(shared.current)
                } else {
                    None
                }
            })
        }
    }

    /// Get the personality instance associated with the current thread
    #[inline]
    pub fn current_personality<F, R>(f: F) -> Option<R>
    where
        F: FnOnce(&mut Box<dyn Personality>) -> R,
    {
        Self::current_thread()
            .and_then(|thread| unsafe { thread.unsafe_weak() })
            .and_then(|thread| thread.personality.as_mut())
            .map(|v| f(v))
    }

    pub(crate) unsafe fn reschedule() {
        if Self::is_enabled() {
            Cpu::without_interrupts(|| {
                let shared = Self::shared();
                Self::process_timer_events();
                let current = shared.current;
                current.update_statistics();
                let priority = current.as_ref().priority;
                if priority == Priority::Realtime {
                    return;
                }
                if let Some(next) = shared.queue_realtime.dequeue() {
                    Self::switch_context(next);
                } else if let Some(next) = if priority < Priority::High {
                    shared.queue_higher.dequeue()
                } else {
                    None
                } {
                    Self::switch_context(next);
                } else if let Some(next) = if priority < Priority::Normal {
                    shared.queue_normal.dequeue()
                } else {
                    None
                } {
                    Self::switch_context(next);
                } else if let Some(next) = if priority < Priority::Low {
                    shared.queue_lower.dequeue()
                } else {
                    None
                } {
                    Self::switch_context(next);
                } else if current.update(|current| current.quantum.consume()) {
                    if let Some(next) = match priority {
                        Priority::Idle => None,
                        Priority::Low => shared.queue_lower.dequeue(),
                        Priority::Normal => shared.queue_normal.dequeue(),
                        Priority::High => shared.queue_higher.dequeue(),
                        Priority::Realtime => None,
                    } {
                        Self::switch_context(next);
                    }
                }
            })
        }
    }

    pub fn sleep() {
        unsafe {
            Cpu::without_interrupts(|| {
                {
                    let shared = Self::shared();
                    let current = shared.current;
                    current.update_statistics();
                    current.as_ref().attribute.insert(ThreadAttributes::ASLEEP);
                }
                Self::switch_context(Self::next());
            })
        }
    }

    pub fn yield_thread() {
        unsafe {
            Cpu::without_interrupts(|| {
                let shared = Self::shared();
                shared.current.update_statistics();
                Self::switch_context(Self::next());
            })
        }
    }

    /// Spawning asynchronous tasks
    pub fn spawn_async(task: Task) {
        Self::current_thread().unwrap().update(|thread| {
            if thread.executor.is_none() {
                thread.executor = Some(Executor::new());
            }
            thread.executor.as_mut().unwrap().spawn(task);
        });
    }

    /// Performing Asynchronous Tasks
    pub fn perform_tasks() -> ! {
        Self::current_thread().unwrap().update(|thread| {
            thread.executor.as_mut().map(|v| v.run());
        });
        Self::exit();
    }

    pub fn exit() -> ! {
        let current = Self::current_thread().unwrap();
        unsafe {
            current.unsafe_weak().unwrap().exit();
        }
    }

    /// Get the next executable thread from the thread queue
    fn next() -> ThreadHandle {
        let shared = Self::shared();
        // if shared.is_frozen.load(Ordering::SeqCst) {
        //     return None;
        // }
        if let Some(next) = shared.queue_realtime.dequeue() {
            next
        } else if let Some(next) = shared.queue_higher.dequeue() {
            next
        } else if let Some(next) = shared.queue_normal.dequeue() {
            next
        } else if let Some(next) = shared.queue_lower.dequeue() {
            next
        } else {
            shared.idle
        }
    }

    fn enqueue(&mut self, handle: ThreadHandle) {
        match handle.as_ref().priority {
            Priority::Realtime => self.queue_realtime.enqueue(handle).unwrap(),
            Priority::High => self.queue_higher.enqueue(handle).unwrap(),
            Priority::Normal => self.queue_normal.enqueue(handle).unwrap(),
            Priority::Low => self.queue_lower.enqueue(handle).unwrap(),
            _ => unreachable!(),
        }
    }

    fn retire(handle: ThreadHandle) {
        let shared = Self::shared();
        let thread = handle.as_ref();
        if thread.priority == Priority::Idle {
            return;
        } else if thread.attribute.contains(ThreadAttributes::ZOMBIE) {
            drop(thread);
            ThreadPool::drop_thread(handle);
        } else if thread.attribute.test_and_clear(ThreadAttributes::AWAKE) {
            thread.attribute.remove(ThreadAttributes::ASLEEP);
            shared.enqueue(handle);
        } else if thread.attribute.contains(ThreadAttributes::ASLEEP) {
            thread.attribute.remove(ThreadAttributes::QUEUED);
        } else {
            shared.enqueue(handle);
        }
    }

    /// Add thread to the queue
    fn add(handle: ThreadHandle) {
        let shared = Self::shared();
        let thread = handle.as_ref();
        if thread.priority == Priority::Idle || thread.attribute.contains(ThreadAttributes::ZOMBIE)
        {
            return;
        }
        if !thread.attribute.test_and_set(ThreadAttributes::QUEUED) {
            if thread.attribute.test_and_clear(ThreadAttributes::AWAKE) {
                thread.attribute.remove(ThreadAttributes::ASLEEP);
            }
            shared.enqueue(thread.handle);
        }
    }

    pub fn schedule_timer(event: TimerEvent) -> Result<(), TimerEvent> {
        unsafe {
            Cpu::without_interrupts(|| {
                let shared = Self::shared();
                shared.timer_events.push(event);
                shared
                    .timer_events
                    .sort_by(|a, b| a.timer.deadline.cmp(&b.timer.deadline));

                // Self::process_timer_event();
            });
            Ok(())
        }
    }

    unsafe fn process_timer_events() {
        Cpu::assert_without_interrupt();

        let shared = Self::shared();
        while let Some(event) = shared.timer_events.first() {
            if event.until() {
                break;
            } else {
                shared.timer_events.remove(0).fire();
            }
        }
    }

    /// Returns whether or not the thread scheduler is working.
    fn is_enabled() -> bool {
        unsafe { &SCHEDULER }.is_some() && SCHEDULER_ENABLED.load(Ordering::SeqCst)
    }

    #[track_caller]
    unsafe fn switch_context(next: ThreadHandle) {
        Cpu::assert_without_interrupt();

        let shared = Self::shared();
        let current = shared.current;
        if current == next {
            return;
        }

        //-//-//-//-//
        shared.retired = Some(current);
        shared.current = next;

        {
            let current = current.unsafe_weak().unwrap();
            let next = &next.unsafe_weak().unwrap().context;
            current.context.switch(next);
        }

        let current = shared.current;
        //-//-//-//-//

        current.update(|thread| {
            thread.attribute.remove(ThreadAttributes::AWAKE);
            thread.attribute.remove(ThreadAttributes::ASLEEP);
            thread.measure.store(Timer::measure().0, Ordering::SeqCst);
        });

        let retired = shared.retired.unwrap();
        shared.retired = None;
        Scheduler::retire(retired);
    }

    fn spawn_f(
        start: ThreadStart,
        args: usize,
        name: &str,
        options: SpawnOption,
    ) -> Option<ThreadHandle> {
        let pid = if options.raise_pid {
            ProcessId::next()
        } else {
            Self::current_pid().unwrap_or(ProcessId(0))
        };
        let thread = RawThread::new(
            pid,
            options.priority,
            name,
            Some(start),
            args,
            options.personality,
        );
        let thread = {
            let handle = thread.handle;
            ThreadPool::shared().add(Box::new(thread));
            handle
        };
        Self::add(thread);
        Some(thread)
    }
}

#[no_mangle]
pub unsafe extern "C" fn sch_setup_new_thread() {
    let shared = Scheduler::shared();
    let current = shared.current;
    current.update(|thread| {
        thread.measure.store(Timer::measure().0, Ordering::SeqCst);
    });
    if let Some(retired) = shared.retired {
        shared.retired = None;
        Scheduler::retire(retired);
    }
}

#[derive(Default)]
struct ThreadPool {
    data: BTreeMap<ThreadHandle, Arc<UnsafeCell<Box<RawThread>>>>,
}

impl ThreadPool {
    #[inline]
    #[track_caller]
    fn synchronized<F, R>(f: F) -> R
    where
        F: FnOnce() -> R,
    {
        unsafe { Cpu::without_interrupts(|| f()) }
    }

    #[inline]
    #[track_caller]
    fn shared<'a>() -> &'a mut Self {
        &mut Scheduler::shared().pool
    }

    #[inline]
    fn add(&mut self, thread: Box<RawThread>) {
        Self::synchronized(|| {
            let handle = thread.handle;
            self.data.insert(handle, Arc::new(UnsafeCell::new(thread)));
        });
    }

    #[inline]
    fn drop_thread(handle: ThreadHandle) {
        Self::synchronized(|| {
            let shared = Self::shared();
            let _removed = shared.data.remove(&handle).unwrap();
        });
    }

    #[inline]
    unsafe fn unsafe_weak<'a>(&self, key: ThreadHandle) -> Option<&'a mut Box<RawThread>> {
        Self::synchronized(|| self.data.get(&key).map(|v| &mut *(&*Arc::as_ptr(v)).get()))
    }

    #[inline]
    fn get<'a>(&self, key: &ThreadHandle) -> Option<&'a Box<RawThread>> {
        Self::synchronized(|| self.data.get(key).map(|v| v.clone().get()))
            .map(|thread| unsafe { &(*thread) })
    }

    #[inline]
    fn get_mut<F, R>(&mut self, key: &ThreadHandle, f: F) -> Option<R>
    where
        F: FnOnce(&mut RawThread) -> R,
    {
        let thread = Self::synchronized(move || self.data.get_mut(key).map(|v| v.clone()));
        thread.map(|thread| unsafe {
            let thread = thread.get();
            f(&mut *thread)
        })
    }
}

pub struct SpawnOption {
    pub priority: Priority,
    pub raise_pid: bool,
    pub personality: Option<Box<dyn Personality>>,
}

impl SpawnOption {
    #[inline]
    pub const fn new() -> Self {
        Self {
            priority: Priority::Normal,
            raise_pid: false,
            personality: None,
        }
    }

    #[inline]
    pub const fn with_priority(priority: Priority) -> Self {
        Self {
            priority,
            raise_pid: false,
            personality: None,
        }
    }

    #[inline]
    pub fn personality(mut self, personality: Box<dyn Personality>) -> Self {
        self.personality = Some(personality);
        self
    }

    #[inline]
    pub fn spawn_f(self, start: fn(usize), args: usize, name: &str) -> Option<ThreadHandle> {
        Scheduler::spawn_f(start, args, name, self)
    }

    #[inline]
    pub fn spawn(mut self, start: fn(usize), args: usize, name: &str) -> Option<ThreadHandle> {
        self.raise_pid = true;
        Scheduler::spawn_f(start, args, name, self)
    }
}

static mut TIMER_SOURCE: Option<&'static dyn TimerSource> = None;

pub trait TimerSource {
    fn measure(&self) -> TimeSpec;

    fn from_duration(&self, val: Duration) -> TimeSpec;

    fn to_duration(&self, val: TimeSpec) -> Duration;
}

#[derive(Debug, Copy, Clone, Default)]
pub struct Timer {
    deadline: TimeSpec,
}

impl Timer {
    pub const JUST: Timer = Timer {
        deadline: TimeSpec(0),
    };

    #[inline]
    pub fn new(duration: Duration) -> Self {
        let timer = Self::timer_source();
        Timer {
            deadline: timer.measure() + duration.into(),
        }
    }

    pub fn epsilon() -> Self {
        let timer = Self::timer_source();
        Timer {
            deadline: timer.measure() + TimeSpec::EPSILON,
        }
    }

    #[inline]
    pub const fn is_just(&self) -> bool {
        self.deadline.0 == 0
    }

    #[inline]
    pub fn until(&self) -> bool {
        if self.is_just() {
            false
        } else {
            let timer = Self::timer_source();
            self.deadline > timer.measure()
        }
    }

    #[inline]
    pub fn repeat_until<F>(&self, mut f: F)
    where
        F: FnMut(),
    {
        while self.until() {
            f()
        }
    }

    #[inline]
    pub(crate) unsafe fn set_timer(source: &'static dyn TimerSource) {
        TIMER_SOURCE = Some(source);
    }

    fn timer_source() -> &'static dyn TimerSource {
        unsafe { TIMER_SOURCE.unwrap() }
    }

    #[track_caller]
    pub fn sleep(duration: Duration) {
        if Scheduler::is_enabled() {
            let timer = Timer::new(duration);
            let event = TimerEvent::one_shot(timer);
            let _ = Scheduler::schedule_timer(event);
            Scheduler::sleep();
        } else {
            panic!("Scheduler unavailable");
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
    pub fn measure() -> TimeSpec {
        Self::timer_source().measure()
    }

    #[inline]
    pub fn monotonic() -> Duration {
        Self::measure().into()
    }

    #[inline]
    fn timespec_to_duration(val: TimeSpec) -> Duration {
        Self::timer_source().to_duration(val)
    }

    #[inline]
    fn duration_to_timespec(val: Duration) -> TimeSpec {
        Self::timer_source().from_duration(val)
    }
}

#[repr(transparent)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct TimeSpec(pub usize);

impl TimeSpec {
    pub const EPSILON: Self = Self(1);
}

impl Add<TimeSpec> for TimeSpec {
    type Output = Self;
    #[inline]
    fn add(self, rhs: TimeSpec) -> Self::Output {
        TimeSpec(self.0 + rhs.0)
    }
}

impl From<TimeSpec> for Duration {
    #[inline]
    fn from(val: TimeSpec) -> Duration {
        Timer::timespec_to_duration(val)
    }
}

impl From<Duration> for TimeSpec {
    #[inline]
    fn from(val: Duration) -> TimeSpec {
        Timer::duration_to_timespec(val)
    }
}

pub struct TimerEvent {
    timer: Timer,
    timer_type: TimerType,
}

#[derive(Debug, Copy, Clone)]
pub enum TimerType {
    OneShot(ThreadHandle),
    Window(WindowHandle, usize),
}

#[allow(dead_code)]
impl TimerEvent {
    pub fn one_shot(timer: Timer) -> Self {
        Self {
            timer,
            timer_type: TimerType::OneShot(Scheduler::current_thread().unwrap()),
        }
    }

    pub fn window(window: WindowHandle, timer_id: usize, timer: Timer) -> Self {
        Self {
            timer,
            timer_type: TimerType::Window(window, timer_id),
        }
    }

    pub fn until(&self) -> bool {
        self.timer.until()
    }

    pub fn fire(self) {
        match self.timer_type {
            TimerType::OneShot(thread) => thread.wake(),
            TimerType::Window(window, timer_id) => {
                window.post(WindowMessage::Timer(timer_id)).unwrap()
            }
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd, Eq, Ord)]
pub struct ProcessId(usize);

impl ProcessId {
    #[inline]
    fn next() -> Self {
        static NEXT_ID: AtomicUsize = AtomicUsize::new(1);
        Self(Cpu::interlocked_increment(&NEXT_ID))
    }
}

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd, Eq, Ord)]
pub struct ThreadHandle(NonZeroUsize);

impl ThreadHandle {
    #[inline]
    pub fn new(val: usize) -> Option<Self> {
        NonZeroUsize::new(val).map(|x| Self(x))
    }

    /// Acquire the next thread ID
    #[inline]
    fn next() -> Self {
        static NEXT_ID: AtomicUsize = AtomicUsize::new(1);
        Self::new(Cpu::interlocked_increment(&NEXT_ID)).unwrap()
    }

    #[inline]
    pub const fn as_usize(&self) -> usize {
        self.0.get()
    }

    #[inline]
    #[track_caller]
    fn update<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut RawThread) -> R,
    {
        let shared = ThreadPool::shared();
        shared.get_mut(self, f).unwrap()
    }

    #[inline]
    fn get<'a>(&self) -> Option<&'a Box<RawThread>> {
        let shared = ThreadPool::shared();
        shared.get(self)
    }

    #[inline]
    #[track_caller]
    fn as_ref<'a>(&self) -> &'a RawThread {
        self.get().unwrap()
    }

    #[inline]
    #[track_caller]
    unsafe fn unsafe_weak<'a>(&self) -> Option<&'a mut Box<RawThread>> {
        let shared = ThreadPool::shared();
        shared.unsafe_weak(*self)
    }

    #[inline]
    pub fn name(&self) -> Option<&str> {
        self.get().and_then(|v| v.name())
    }

    #[inline]
    pub fn wake(&self) {
        self.as_ref().attribute.insert(ThreadAttributes::AWAKE);
        Scheduler::add(*self);
    }

    #[inline]
    pub fn join(&self) -> usize {
        self.get().map(|t| t.sem.wait());
        0
    }

    fn update_statistics(&self) {
        self.update(|thread| {
            let now = Timer::measure().0;
            let then = thread.measure.swap(now, Ordering::SeqCst);
            let diff = now - then;
            thread.cpu_time.fetch_add(diff, Ordering::SeqCst);
            thread.load0.fetch_add(diff as u32, Ordering::SeqCst);
        });
    }
}

#[repr(u8)]
#[non_exhaustive]
#[derive(Debug, Copy, Clone, PartialEq, Ord, PartialOrd, Eq)]
pub enum Priority {
    Idle = 0,
    Low,
    Normal,
    High,
    Realtime,
}

impl Priority {
    pub fn is_useful(self) -> bool {
        match self {
            Priority::Idle => false,
            _ => true,
        }
    }
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
struct Quantum {
    current: u8,
    default: u8,
}

impl Quantum {
    const fn new(value: u8) -> Self {
        Quantum {
            current: value,
            default: value,
        }
    }

    #[allow(dead_code)]
    fn reset(&mut self) {
        self.current = self.default;
    }

    fn consume(&mut self) -> bool {
        if self.current > 1 {
            self.current -= 1;
            false
        } else {
            self.current = self.default;
            true
        }
    }
}

impl From<Priority> for Quantum {
    fn from(priority: Priority) -> Self {
        match priority {
            Priority::High => Quantum::new(10),
            Priority::Normal => Quantum::new(5),
            Priority::Low => Quantum::new(1),
            _ => Quantum::new(1),
        }
    }
}

const THREAD_NAME_LENGTH: usize = 32;

type ThreadStart = fn(usize) -> ();

#[allow(dead_code)]
struct RawThread {
    /// Architecture-specific context data
    context: CpuContextData,
    stack: Option<Box<[u8]>>,

    // IDs
    pid: ProcessId,
    handle: ThreadHandle,

    // Properties
    sem: Semaphore,
    personality: Option<Box<dyn Personality>>,
    attribute: AtomicBitflags<ThreadAttributes>,
    priority: Priority,
    quantum: Quantum,

    // Statistics
    measure: AtomicUsize,
    cpu_time: AtomicUsize,
    load0: AtomicU32,
    load: AtomicU32,

    // Executor
    executor: Option<Executor>,

    // Thread Name
    name: [u8; THREAD_NAME_LENGTH],
}

bitflags! {
    struct ThreadAttributes: usize {
        const QUEUED    = 0b0000_0000_0000_0001;
        const ASLEEP    = 0b0000_0000_0000_0010;
        const AWAKE     = 0b0000_0000_0000_0100;
        const ZOMBIE    = 0b0000_0000_0000_1000;
    }
}

impl Into<usize> for ThreadAttributes {
    fn into(self) -> usize {
        self.bits()
    }
}

impl AtomicBitflags<ThreadAttributes> {
    fn to_char(&self) -> char {
        if self.contains(ThreadAttributes::ZOMBIE) {
            'Z'
        } else if self.contains(ThreadAttributes::AWAKE) {
            'W'
        } else if self.contains(ThreadAttributes::ASLEEP) {
            'S'
        } else if self.contains(ThreadAttributes::QUEUED) {
            'R'
        } else {
            '-'
        }
    }
}

use core::fmt;
impl fmt::Display for AtomicBitflags<ThreadAttributes> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_char())
    }
}

impl RawThread {
    fn new(
        pid: ProcessId,
        priority: Priority,
        name: &str,
        start: Option<ThreadStart>,
        arg: usize,
        personality: Option<Box<dyn Personality>>,
    ) -> Self {
        let handle = ThreadHandle::next();

        let mut name_array = [0; THREAD_NAME_LENGTH];
        Self::set_name_array(&mut name_array, name);

        let mut thread = Self {
            context: CpuContextData::new(),
            stack: None,
            pid,
            handle,
            sem: Semaphore::new(0),
            attribute: AtomicBitflags::empty(),
            priority,
            quantum: Quantum::from(priority),
            measure: AtomicUsize::new(0),
            cpu_time: AtomicUsize::new(0),
            load0: AtomicU32::new(0),
            load: AtomicU32::new(0),
            executor: None,
            personality,
            name: name_array,
        };
        if let Some(start) = start {
            unsafe {
                let size_of_stack = CpuContextData::SIZE_OF_STACK;
                let mut stack = Vec::with_capacity(size_of_stack);
                stack.resize(size_of_stack, 0);
                let stack = stack.into_boxed_slice();
                thread.stack = Some(stack);
                let stack = thread.stack.as_mut().unwrap().as_mut_ptr() as *mut c_void;
                thread
                    .context
                    .init(stack.add(size_of_stack), start as usize, arg);
            }
        }
        thread
    }

    #[inline]
    fn exit(&mut self) -> ! {
        self.sem.signal();
        self.personality.as_mut().map(|v| v.on_exit());
        self.personality = None;

        // TODO:
        Timer::sleep(Duration::from_secs(2));
        self.attribute.insert(ThreadAttributes::ZOMBIE);
        Scheduler::sleep();
        unreachable!()
    }

    #[inline]
    fn set_name_array(array: &mut [u8; THREAD_NAME_LENGTH], name: &str) {
        let mut i = 1;
        for c in name.bytes() {
            if i >= THREAD_NAME_LENGTH {
                break;
            }
            array[i] = c;
            i += 1;
        }
        array[0] = i as u8 - 1;
    }

    // fn set_name(&mut self, name: &str) {
    //     RawThread::set_name_array(&mut self.name, name);
    // }

    fn name<'a>(&self) -> Option<&'a str> {
        let len = self.name[0] as usize;
        match len {
            0 => None,
            _ => core::str::from_utf8(unsafe { core::slice::from_raw_parts(&self.name[1], len) })
                .ok(),
        }
    }
}

struct ThreadQueue(Fifo<usize>);

impl ThreadQueue {
    fn with_capacity(capacity: usize) -> Self {
        Self(Fifo::new(capacity))
    }

    fn dequeue(&mut self) -> Option<ThreadHandle> {
        unsafe { self.0.dequeue().and_then(|v| ThreadHandle::new(v)) }
    }

    fn enqueue(&mut self, data: ThreadHandle) -> Result<(), ()> {
        unsafe { self.0.enqueue(data.as_usize()).map_err(|_| ()) }
    }
}
