//! Minimal SBI implementation for minios
//!
//! This is a minimal SBI implementation that provides only the necessary functions for minios to run on RISC-V virt machine.
#![cfg_attr(not(test), no_std)]

use crate::syscon::Syscon;
use crate::uart::Uart16550;
use core::arch::{asm, naked_asm};
use core::sync::atomic::{Ordering, compiler_fence};
use riscv::XLEN;
use riscv::csr::pmp::{PmpAddressMode, PmpConfig, PmpIndex};
use riscv::csr::{CSR, VectorMode};

pub mod sbi_ecall;
pub mod syscon;
pub mod timer;
pub mod trap;
pub mod uart;

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => {{
        #[allow(unused_imports)]
        use core::fmt::Write;
        let _ = write!(StdOut::stdout(), $($arg)*);
    }};
}

#[macro_export]
macro_rules! println {
    ($($arg:tt)*) => {{
        #[allow(unused_imports)]
        use core::fmt::Write;
        let _ = writeln!(StdOut::stdout(), $($arg)*);
    }};
}

/// Initialize the MiniSBI.
pub unsafe fn init(hart_id: usize) {
    unsafe {
        if hart_id != 0 {
            // TODO: support multiple harts
            CSR::MSTATUS.write(0);
            loop {
                asm!("wfi", options(nomem, nostack));
            }
        }

        Uart16550::init(0x1000_0000);

        trap::init();
        timer::init();

        println!("MiniSBI v0.0");

        let misa = CSR::MISA.read();
        let mxl = (misa >> (XLEN - 2)) & 0x3;
        print!("misa:       {:08x} RV{}", misa, mxl * 32);
        for i in 0..26 {
            let ext = ((misa >> i) & 1) != 0;
            if ext {
                print!("{}", (b'A' + i) as char);
            }
        }
        println!("");

        println!("mvendorid:  {:08x}", CSR::MVENDORID.read());
        println!("marchid:    {:08x}", CSR::MARCHID.read());
        println!("mimpid:     {:08x}", CSR::MIMPID.read());
        println!("mhartid:    {:08x}", CSR::MHARTID.read());
        println!("");

        PmpIndex::Pmp0.write(
            PmpConfig::new(true, true, true, PmpAddressMode::Tor, false),
            0x4000_0000,
        );

        for pmp in PmpIndex::iter() {
            if let Some((cfg, addr)) = pmp.read() {
                println!(
                    "pmp{}: {}{}{}{} addr:{:08x} mode:{:?}",
                    pmp as usize,
                    if cfg.is_locked() { "L" } else { "-" },
                    if cfg.is_readable() { "R" } else { "-" },
                    if cfg.is_writable() { "W" } else { "-" },
                    if cfg.is_executable() { "X" } else { "-" },
                    addr,
                    cfg.addr_mode(),
                );
            } else {
                continue;
            }
        }

        println!("Next: S-mode");
        println!("");

        compiler_fence(Ordering::SeqCst);

        // Set MPP to S mode and switch to S mode
        CSR::MSTATUS.clear(0x1800);
        CSR::MSTATUS.set(0x0800);

        asm!(
            "    la {0}, 100f",
            "    csrw mepc, {0}",
            "    mret",
            "",
            "100:",
                lateout(reg) _,
        );
    }
}

#[inline(always)]
pub(crate) fn no_op() {
    unsafe {
        asm!("nop", options(nomem, nostack));
    }
}

pub struct StdOut;

impl StdOut {
    #[inline]
    pub fn stdout() -> Self {
        Self
    }

    #[inline]
    pub fn write_byte(&mut self, byte: u8) {
        Uart16550::shared().write_byte(byte);
    }
}

impl core::fmt::Write for StdOut {
    #[inline]
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        for b in s.bytes() {
            self.write_byte(b);
        }
        Ok(())
    }
}

pub struct StdIn;

impl StdIn {
    #[inline]
    pub fn stdin() -> Self {
        Self
    }

    #[inline]
    pub fn read_byte(&mut self) -> Option<u8> {
        Uart16550::shared().read_byte()
    }
}
