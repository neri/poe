//! Programmable Interrupt Controller i8259

use super::cpu::{Idt, KERNEL_DSEL};
use super::vm86::{VM86, X86StackContext};
use crate::platform::Platform;
use crate::*;
use core::{
    arch::{asm, global_asm},
    cell::UnsafeCell,
    num::NonZeroUsize,
};
use paste::paste;
use seq_macro::seq;
use x86::prot::{DPL0, InterruptVector};

static mut PIC: UnsafeCell<Pic> = UnsafeCell::new(Pic::new());

pub type IrqHandler = fn(Irq) -> ();

/// Programmable Interrupt Controller i8259
pub struct Pic {
    master: I8259Device,
    slave: I8259Device,
    chain_eoi: u8,
    old_imr: u32,
    redirect_bitmap: u32,
    redirect_table: [u8; 16],
    icw1: u8,
    slave_id: u8,
    icw4_m: u8,
    icw4_s: u8,
    idt: [usize; 16],
}

macro_rules! handle_master_irq {
    ($local_irq:expr) => {
        paste! {
            unsafe extern "C" {
                fn [<irq_m $local_irq>]() -> !;
            }

            global_asm!(
                "{label}:",
                "cld",
                "push 0",
                "push 0",
                "pushad",

                ".byte 0x06", // push es
                ".byte 0x1e", // push ds
                ".byte 0x0f, 0xa0", // push fs
                ".byte 0x0f, 0xa8", // push gs

                "mov eax, {dsel}",
                "mov ds, eax",
                "mov es, eax",

                "mov ecx, {local_irq}",
                "mov edx, esp",
                "call {handler}",

                ".byte 0x0f, 0xa9", // pop gs
                ".byte 0x0f, 0xa1", // pop fs
                ".byte 0x1f", // pop ds
                ".byte 0x07", // pop es
                "popad",
                "add esp, 8",
                "iretd",
                local_irq = const $local_irq,
                label = sym [<irq_m $local_irq>],
                handler = sym pic_handle_master_irq,
                dsel = const KERNEL_DSEL.as_usize(),
            );
        }
    };
}

macro_rules! handle_slave_irq {
    ($local_irq:expr) => {
        paste! {
            unsafe extern "C" {
                fn [<irq_s $local_irq>]() -> !;
            }

            global_asm!(
                "{label}:",
                "cld",
                "push 0",
                "push 0",
                "pushad",

                ".byte 0x06", // push es
                ".byte 0x1e", // push ds
                ".byte 0x0f, 0xa0", // push fs
                ".byte 0x0f, 0xa8", // push gs

                "mov eax, {dsel}",
                "mov ds, eax",
                "mov es, eax",

                "mov ecx, {local_irq}",
                "mov edx, esp",
                "call {handler}",

                ".byte 0x0f, 0xa9", // pop gs
                ".byte 0x0f, 0xa1", // pop fs
                ".byte 0x1f", // pop ds
                ".byte 0x07", // pop es
                "popad",
                "add esp, 8",
                "iretd",
                local_irq = const $local_irq,
                label = sym [<irq_s $local_irq>],
                handler = sym pic_handle_slave_irq,
                dsel = const KERNEL_DSEL.as_usize(),
            );
        }
    };
}

handle_master_irq!(0);
handle_master_irq!(1);
handle_master_irq!(2);
handle_master_irq!(3);
handle_master_irq!(4);
handle_master_irq!(5);
handle_master_irq!(6);
handle_master_irq!(7);

handle_slave_irq!(0);
handle_slave_irq!(1);
handle_slave_irq!(2);
handle_slave_irq!(3);
handle_slave_irq!(4);
handle_slave_irq!(5);
handle_slave_irq!(6);
handle_slave_irq!(7);

#[unsafe(no_mangle)]
pub unsafe extern "fastcall" fn pic_handle_master_irq(irq: Irq, regs: &mut X86StackContext) {
    unsafe {
        let shared = Pic::shared();

        if shared.redirect_bitmap & (1 << irq.0 as usize) != 0 {
            VM86::redirect_interrupt(InterruptVector(shared.redirect_table[irq.0 as usize]), regs);
        } else {
            NonZeroUsize::new(*shared.idt.get_unchecked(irq.0 as usize)).map(|v| {
                let f: IrqHandler = core::mem::transmute(v.get());
                f(irq);
            });

            // EOI
            shared.master.write_a0(0x60 + irq.local_number());

            // if irq == Irq(0) {
            //     Scheduler::reschedule();
            // }
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "fastcall" fn pic_handle_slave_irq(lirq: u8, regs: &mut X86StackContext) {
    unsafe {
        let shared = Pic::shared();
        let girq = Irq(lirq + 8);

        if shared.redirect_bitmap & (1 << girq.0 as usize) != 0 {
            VM86::redirect_interrupt(
                InterruptVector(shared.redirect_table[girq.0 as usize]),
                regs,
            );
        } else {
            NonZeroUsize::new(*shared.idt.get_unchecked(girq.0 as usize)).map(|v| {
                let f: IrqHandler = core::mem::transmute(v.get());
                f(girq);
            });

            // EOI
            shared.slave.write_a0(0x60 + girq.local_number());
            if shared.slave.read_isr() == 0 {
                shared.master.write_a0(shared.chain_eoi);
            }
        }
    }
}

impl Pic {
    const fn new() -> Self {
        Self {
            master: I8259Device::zero(),
            slave: I8259Device::zero(),
            chain_eoi: 0,
            old_imr: 0,
            redirect_bitmap: 0,
            redirect_table: [0; 16],
            icw1: 0,
            slave_id: 0,
            icw4_m: 0,
            icw4_s: 0,
            idt: [0; 16],
        }
    }

    pub(crate) unsafe fn init(platform: Platform) {
        unsafe {
            let shared = Self::shared();
            match platform {
                Platform::PcBios => {
                    shared.master.set_addrs(0x0020, 0x0021);
                    shared.slave.set_addrs(0x00a0, 0x00a1);
                    shared.redirect_table = [
                        0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f, 0x70, 0x71, 0x72, 0x73,
                        0x74, 0x75, 0x76, 0x77,
                    ];
                    shared.init_pic(
                        0b00010001,
                        0x02,
                        0b0001_0101,
                        0b0000_0001,
                        0b0111_1111_1111_1010,
                    );
                }
                Platform::Nec98 => {
                    shared.master.set_addrs(0x0000, 0x0002);
                    shared.slave.set_addrs(0x0008, 0x000a);
                    shared.redirect_table = [
                        0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f, 0x10, 0x11, 0x12, 0x13,
                        0x14, 0x15, 0x16, 0x17,
                    ];
                    shared.init_pic(
                        0b00010001,
                        0x07,
                        0b0001_1101,
                        0b0000_1001,
                        0b0111_1111_0111_1110,
                    );
                }
                Platform::FmTowns => {
                    shared.master.set_addrs(0x0000, 0x0002);
                    shared.slave.set_addrs(0x0010, 0x0012);
                    shared.redirect_table = [
                        0x40, 0x41, 0x42, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48, 0x49, 0x4a, 0x4b,
                        0x4c, 0x4d, 0x4e, 0x4f,
                    ];
                    shared.init_pic(
                        0b00011001,
                        0x07,
                        0b0001_1101,
                        0b0000_1001,
                        0b0000_0000_0000_0000,
                    );
                }
                _ => unreachable!(),
            }

            seq!(N in 0..8 {
                Idt::register(
                    Irq(N).as_vec(),
                    irq_m~N as usize,
                    DPL0,
                    true,
                );
            });
            seq!(N in 0..8 {
                Idt::register(
                    Irq(N + 8).as_vec(),
                    irq_s~N as usize,
                    DPL0,
                    true,
                );
            });
        }
    }

    #[inline]
    unsafe fn shared<'a>() -> &'a mut Self {
        unsafe { (&mut *(&raw mut PIC)).get_mut() }
    }

    /// Init PICs
    ///
    /// - parameters icw1, slave_id, icw4_master, icw4_slave
    /// - parameters redirect_mask: redirect mask, 1: redirect, 0: use IDT
    #[inline]
    unsafe fn init_pic(
        &mut self,
        icw1: u8,
        slave_id: u8,
        icw4_m: u8,
        icw4_s: u8,
        redirect_mask: u32,
    ) {
        self.icw1 = icw1;
        self.slave_id = slave_id;
        self.icw4_m = icw4_m;
        self.icw4_s = icw4_s;
        unsafe {
            let old_imr0 = self.master.read_imr();
            let old_imr1 = self.slave.read_imr();
            self.old_imr = (old_imr1 as u32) << 8 | old_imr0 as u32;

            self.master.write_imr(u8::MAX);
            self.slave.write_imr(u8::MAX);
            Hal::cpu().no_op();

            let icw3_m = 1 << slave_id;
            self.master.write_a0(icw1);
            self.master.write_a1(Irq::BASE.0);
            self.master.write_a1(icw3_m);
            self.master.write_a1(icw4_m);

            self.slave.write_a0(icw1);
            self.slave.write_a1(Irq::BASE.0 + 8);
            self.slave.write_a1(slave_id);
            self.slave.write_a1(icw4_s);

            let new_imr0 = old_imr0 & !icw3_m & 0x7F;
            let new_imr1 = old_imr1 & 0x7F;
            self.redirect_bitmap = redirect_mask & !self.old_imr;

            self.master.write_imr(new_imr0);
            self.slave.write_imr(new_imr1);

            self.chain_eoi = 0x60 + slave_id;
        }
    }

    pub(crate) unsafe fn exit() {
        unsafe {
            let shared = Self::shared();

            shared.master.write_imr(u8::MAX);
            shared.slave.write_imr(u8::MAX);
            Hal::cpu().no_op();
            Hal::cpu().disable_interrupt();

            let icw3_m = 1 << shared.slave_id;
            shared.master.write_a0(shared.icw1);
            shared.master.write_a1(shared.redirect_table[0]);
            shared.master.write_a1(icw3_m);
            shared.master.write_a1(shared.icw4_m);

            shared.slave.write_a0(shared.icw1);
            shared.slave.write_a1(shared.redirect_table[8]);
            shared.slave.write_a1(shared.slave_id);
            shared.slave.write_a1(shared.icw4_s);

            let old_imr0 = shared.old_imr as u8;
            let old_imr1 = (shared.old_imr >> 8) as u8;
            shared.master.write_imr(old_imr0);
            shared.slave.write_imr(old_imr1);
        }
    }

    /// Register a new IRQ handler
    pub unsafe fn register(irq: Irq, f: IrqHandler) -> Result<(), ()> {
        without_interrupts!(unsafe {
            let shared = Self::shared();
            let irq_index = irq.0 as usize;
            if shared.idt[irq_index] != 0 {
                return Err(());
            }
            shared.redirect_bitmap &= !(1 << irq_index);
            shared.idt[irq_index] = f as usize;
            Self::set_irq_enabled(irq, true);
            Ok(())
        })
    }

    /// Set the redirect flag for a specific IRQ
    #[allow(unused)]
    pub unsafe fn set_redirect(irq: Irq) -> Result<(), ()> {
        without_interrupts!(unsafe {
            let shared = Self::shared();
            let irq_index = irq.0 as usize;
            shared.redirect_bitmap |= 1 << irq_index;
            Self::set_irq_enabled(irq, true);
            Ok(())
        })
    }

    /// Set the IRQ enabled state
    pub unsafe fn set_irq_enabled(irq: Irq, enabled: bool) {
        without_interrupts!(unsafe {
            let shared = Self::shared();
            if irq.is_slave() {
                let local_irq = irq.local_number();
                shared.slave.set_enabled(local_irq, enabled);
            } else {
                let local_irq = irq.local_number();
                shared.master.set_enabled(local_irq, enabled);
            }
        })
    }
}

struct I8259Device {
    a0: u32,
    a1: u32,
}

#[allow(unused)]
impl I8259Device {
    #[inline]
    const fn zero() -> Self {
        Self { a0: 0, a1: 0 }
    }

    #[inline]
    fn set_addrs(&mut self, a0: u32, a1: u32) {
        self.a0 = a0;
        self.a1 = a1;
    }

    #[inline]
    unsafe fn write_a0(&self, val: u8) {
        unsafe {
            asm!(
                "out dx, al",
                in("edx") self.a0,
                in("al") val,
            );
        }
    }

    #[inline]
    unsafe fn write_a1(&self, val: u8) {
        unsafe {
            asm!(
                "out dx, al",
                in("edx") self.a1,
                in("al") val,
            );
        }
    }

    #[inline]
    unsafe fn read_a0(&self) -> u8 {
        let result: u8;
        unsafe {
            asm!(
                "in al, dx",
                in ("edx") self.a0,
                lateout ("al") result,
            );
        }
        result
    }

    #[inline]
    unsafe fn read_a1(&self) -> u8 {
        let result: u8;
        unsafe {
            asm!(
                "in al, dx",
                in ("edx") self.a1,
                lateout ("al") result,
            );
        }
        result
    }

    #[inline]
    unsafe fn read_isr(&self) -> u8 {
        unsafe {
            let result: u8;
            asm!(
                "out dx, al",
                "nop",
                "in al, dx",
                inout("al") 0x0bu8 => result,
                in("edx") self.a0,
            );
            result
        }
    }

    #[inline]
    unsafe fn read_imr(&self) -> u8 {
        unsafe { self.read_a1() }
    }

    #[inline]
    unsafe fn write_imr(&self, val: u8) {
        unsafe { self.write_a1(val) }
    }

    #[inline]
    unsafe fn set_enabled(&self, local_irq: u8, enabled: bool) {
        unsafe {
            let mut imr = self.read_imr();
            if enabled {
                imr &= !(1 << local_irq);
            } else {
                imr |= 1 << local_irq;
            }
            self.write_imr(imr);
        }
    }
}

#[repr(transparent)]
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd, Default)]
pub struct Irq(pub u8);

impl Irq {
    const BASE: InterruptVector = InterruptVector(0x20);
    // const MAX: Irq = Irq(16);

    pub const fn as_vec(self) -> InterruptVector {
        InterruptVector(Self::BASE.0 + self.0)
    }

    pub unsafe fn register(&self, f: IrqHandler) -> Result<(), ()> {
        unsafe { Pic::register(*self, f) }
    }

    pub unsafe fn enable(&self) {
        unsafe { Pic::set_irq_enabled(*self, true) }
    }

    pub unsafe fn disable(&self) {
        unsafe { Pic::set_irq_enabled(*self, false) }
    }

    pub const fn is_slave(&self) -> bool {
        self.0 >= 8
    }

    pub const fn local_number(&self) -> u8 {
        self.0 & 7
    }
}

impl From<Irq> for InterruptVector {
    fn from(irq: Irq) -> InterruptVector {
        irq.as_vec()
    }
}
