//! Mini OS Library

#![cfg_attr(not(test), no_std)]

extern crate alloc;

pub mod arch;
pub mod env;
pub mod io;
pub mod mem;
pub mod platform;
pub mod sync;
pub mod task;

#[allow(unused_imports)]
pub use crate::_prelude_::*;

pub(crate) mod _prelude_ {
    pub use crate::arch::hal::InterruptGuard;
    pub use crate::prelude::*;
}

pub mod prelude {
    pub use alloc::borrow::ToOwned;
    pub use alloc::boxed::Box;
    pub use alloc::collections::BTreeMap;
    pub use alloc::rc::Rc;
    pub use alloc::string::{String, ToString};
    pub use alloc::sync::Arc;
    pub use alloc::vec::Vec;
    pub use core::fmt::Write;

    pub use crate::arch::hal::*;
    pub use crate::env::*;
    pub use crate::io::media::*;
    pub use crate::io::tty::*;
    pub use crate::platform::{Platform, PlatformTrait, RecommendedConsoleMode};
    pub use crate::task::event::*;
    pub use crate::{print, println};
}

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => {{
        #[allow(unused_imports)]
        use core::fmt::Write;
        let _ = write!(System::stdout(), $($arg)*);
    }};
}

#[macro_export]
macro_rules! println {
    ($($arg:tt)*) => {{
        #[allow(unused_imports)]
        use core::fmt::Write;
        let _ = writeln!(System::stdout(), $($arg)*);
    }};
}
