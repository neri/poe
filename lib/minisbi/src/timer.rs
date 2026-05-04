//! CLINT timer implementation
use riscv::csr::CSR;

pub const MSIP: *mut u32 = 0x0200_0000 as *mut u32;

pub const MTIMECMP: *mut u64 = 0x0200_4000 as *mut u64;

pub const MTIME: *mut u64 = 0x0200_bff8 as *mut u64;

pub(crate) unsafe fn init() {
    unsafe {
        _set_mtimecmp(u64::MAX);

        // enable rdtime
        CSR::MCOUNTREN.set(0b010);

        // delegate timer interrupt to S mode
        CSR::MIDELEG.set(0b0010_0010_0010);
    }
}

pub(crate) unsafe fn handle_timer_interrupt() {
    unsafe {
        _set_mtimecmp(u64::MAX);

        // set STIP
        CSR::MIP.set(1 << 5);
    }
}

pub fn set_timer(timer_value: u64) {
    unsafe {
        // enable MTIE and STIE
        CSR::MIE.set(0b1010_0000);

        // clear STIP
        CSR::MIP.clear(1 << 5);

        _set_mtimecmp(timer_value);
    }
}

unsafe fn _set_mtimecmp(timer_value: u64) {
    unsafe {
        if cfg!(target_arch = "riscv32") {
            let p = MTIMECMP as *mut u32;
            p.add(1).write_volatile(u32::MAX);
            p.write_volatile(timer_value as u32);
            p.add(1).write_volatile((timer_value >> 32) as u32);
        } else {
            MTIMECMP.write_volatile(timer_value);
        }
    }
}
