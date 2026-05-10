//! Arch for x86

mod hal_x86;
#[allow(unused_imports)]
pub use hal_x86::*;

pub mod bits;
pub mod cpu;
pub mod lomem;

#[cfg(target_arch = "x86")]
pub mod x86_32 {
    pub mod gdt32;
    pub use gdt32 as gdt;
    pub mod idt32;
    pub use idt32 as idt;
    pub mod setjmp;
    pub mod vm86;
}

#[cfg(target_arch = "x86")]
pub use x86_32::*;
