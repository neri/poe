//! Mini OS Library

#![cfg_attr(not(test), no_std)]
// #![feature(cfg_select)]
#![feature(negative_impls)]

extern crate alloc;

pub mod arch;
pub mod env;
pub mod io;
pub mod mem;
pub mod platform;
pub mod sync;

#[allow(unused_imports)]
pub use crate::_prelude_::*;

pub(crate) mod _prelude_ {
    pub use crate::arch::InterruptGuard;
    pub use crate::prelude::*;

    pub use core::option::Option::{self, *};
}

pub mod prelude {
    pub use crate::{
        arch::hal::*,
        env::*,
        io::{media::*, tty::*},
    };
    pub use crate::{print, println};
    pub use alloc::{
        borrow::ToOwned,
        boxed::Box,
        collections::BTreeMap,
        rc::Rc,
        string::{String, ToString},
        sync::Arc,
        vec::Vec,
    };
    pub use core::fmt::Write;
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
