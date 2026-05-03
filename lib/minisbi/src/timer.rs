//! SBI Timer

use riscv::csr::CSR;

pub(crate) unsafe fn init() {
    unsafe {
        _set_timer(u64::MAX);

        // enable rdtime
        CSR::MCOUNTREN.set(0b010);

        // delegate timer interrupt to S mode
        CSR::MIDELEG.set(0b0010_0010_0010);

        CSR::MIE.set(0b1010_0000); // enable MTIE and STIE
    }
}

pub(crate) unsafe fn handle_timer_interrupt() {
    unsafe {
        _set_timer(u64::MAX);
        CSR::MIP.set(1 << 5);
    }
}

pub fn set_timer(timer_value: u64) {
    unsafe {
        CSR::MIP.clear(1 << 5);

        _set_timer(u64::MAX);
        _set_timer(timer_value);
    }
}

unsafe fn _set_timer(timer_value: u64) {
    unsafe {
        let p = 0x0200_4000 as *mut u32;
        p.write_volatile(timer_value as u32);
        p.add(1).write_volatile((timer_value >> 32) as u32);
    }
}
