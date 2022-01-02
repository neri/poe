#![no_std]
#![feature(alloc_error_handler)]
#![feature(associated_type_bounds)]
#![feature(const_fn_trait_bound)]
#![feature(core_intrinsics)]
#![feature(option_result_contains)]

use arch::cpu::Cpu;
use core::fmt::Write;
use core::panic::PanicInfo;
use system::System;

pub mod arch;
pub mod audio;
pub mod fonts;
pub mod fs;
pub mod io;
pub mod mem;
pub mod rt;
pub mod sync;
pub mod system;
pub mod task;
pub mod util;
pub mod window;

extern crate alloc;

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => {
        write!(System::stdout(), $($arg)*).unwrap()
    };
}

#[macro_export]
macro_rules! println {
    ($fmt:expr) => {
        print!(concat!($fmt, "\r\n"))
    };
    ($fmt:expr, $($arg:tt)*) => {
        print!(concat!($fmt, "\r\n"), $($arg)*)
    };
}

#[macro_export]
macro_rules! entry {
    ($path:path) => {
        #[inline]
        #[no_mangle]
        extern "fastcall" fn _start(info: &toeboot::BootInfo) -> ! {
            let f: fn() = $path;
            unsafe { system::System::init(info, f) }
        }
    };
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    let _ = write!(System::em_console(), "{}", info);
    unsafe { Cpu::stop() };
}
