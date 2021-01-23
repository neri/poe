#![no_std]
#![feature(asm)]
#![feature(abi_x86_interrupt)]
#![feature(global_asm)]
#![feature(alloc_error_handler)]

use core::fmt::Write;
use core::panic::PanicInfo;
use graphics::emcon::EmConsole;
use system::System;

pub mod arch;
pub mod fonts;
pub mod graphics;
pub mod io;
pub mod mem;
pub mod system;
pub mod task;
pub mod window;

extern crate alloc;

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => {
        write!(stdout(), $($arg)*).unwrap()
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
        pub fn _start(info: &bootprot::BootInfo) {
            let f: fn() = $path;
            unsafe { system::System::init(info, f) }
        }
    };
}

pub fn kernel_halt() {
    unsafe {
        asm!("hlt");
    }
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    println!("{}", info);
    loop {
        unsafe {
            asm!("hlt");
        }
    }
}

#[inline]
pub fn stdout<'a>() -> &'a mut EmConsole {
    System::em_console()
}
