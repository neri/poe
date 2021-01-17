// MEG-OS ToE

#![no_std]
#![feature(asm)]

use core::panic::PanicInfo;

pub mod fonts;
pub mod graphics;
pub mod mem;
pub mod system;

// #[macro_export]
// macro_rules! print {
//     ($($arg:tt)*) => {
//         write!(stdout(), $($arg)*).unwrap()
//     };
// }

// #[macro_export]
// macro_rules! println {
//     ($fmt:expr) => {
//         print!(concat!($fmt, "\r\n"))
//     };
//     ($fmt:expr, $($arg:tt)*) => {
//         print!(concat!($fmt, "\r\n"), $($arg)*)
//     };
// }

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
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}
