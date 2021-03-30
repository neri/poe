// Runtime Environment

pub mod megos;

use crate::task::scheduler::*;
use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;

static mut RE: RuntimeEnvironment = RuntimeEnvironment::new();

#[derive(Debug, Default, Copy, Clone, Ord, PartialOrd, Eq, PartialEq)]
pub struct ProcessId(pub usize);

pub struct RuntimeEnvironment {
    image_loaders: Vec<Box<dyn BinaryRecognizer>>,
}

impl RuntimeEnvironment {
    const fn new() -> Self {
        Self {
            image_loaders: Vec::new(),
        }
    }

    pub(crate) unsafe fn init() {
        let shared = Self::shared();

        shared.image_loaders.push(megos::WasmRecognizer::new());
    }

    fn shared() -> &'static mut Self {
        unsafe { &mut RE }
    }

    // pub(crate) fn raise_pid() -> ProcessId {
    //     static NEXT_PID: AtomicUsize = AtomicUsize::new(1);
    //     let pid = ProcessId(NEXT_PID.fetch_add(1, Ordering::SeqCst));

    //     // TODO:

    //     pid
    // }

    pub fn recognize(blob: &[u8]) -> Option<Box<dyn BinaryLoader>> {
        let shared = Self::shared();
        for recognizer in &shared.image_loaders {
            if let Some(loader) = recognizer.recognize(blob) {
                return Some(loader);
            }
        }
        None
    }

    pub fn exit(exit_code: usize) -> ! {
        let _ = exit_code;
        Scheduler::exit();
    }

    // pub unsafe fn invoke_legacy(context: &LegacyAppContext) -> ! {
    //     Cpu::invoke_legacy(context);
    // }
}

pub trait Personality {
    /// Gets the current personality context
    fn context(&mut self) -> PersonalityContext;

    fn on_exit(&mut self);
}

pub enum PersonalityContext<'a> {
    Native,
    Arlequin(&'a mut megos::ArleRuntime),
}

pub trait BinaryRecognizer {
    fn recognize(&self, blob: &[u8]) -> Option<Box<dyn BinaryLoader>>;
}

pub trait BinaryLoader {
    fn option(&mut self) -> &mut LoadedImageOption;

    fn load(&mut self, blob: &[u8]) -> Result<(), ()>;

    fn invoke_start(self: Box<Self>) -> Option<ThreadHandle>;
}

#[derive(Debug, Default)]
pub struct LoadedImageOption {
    pub name: String,
    pub argv: Vec<String>,
}
