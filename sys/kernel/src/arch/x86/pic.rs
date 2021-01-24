// Programmable Interrupt Controller

use super::cpu::{Cpu, InterruptDescriptorTable, InterruptVector, Selector};
use bootprot::*;
use core::num::NonZeroUsize;

static mut PIC: Pic = Pic::new();

pub type IrqHandler = fn(Irq) -> ();

extern "fastcall" {
    fn asm_handle_irq_table(_: &mut [usize; 16]);
}

pub struct Pic {
    cmd0: u32,
    data0: u32,
    cmd1: u32,
    data1: u32,
    chain_eoi: u8,
    idt: [usize; Irq::MAX.0 as usize],
}

impl Pic {
    const fn new() -> Self {
        Self {
            cmd0: 0,
            data0: 0,
            cmd1: 0,
            data1: 0,
            chain_eoi: 0,
            idt: [0; Irq::MAX.0 as usize],
        }
    }

    pub(super) unsafe fn init(platform: Platform) {
        let shared = Self::shared();
        match platform {
            Platform::PcCompatible => {
                shared.cmd0 = 0x0020;
                shared.data0 = 0x0021;
                shared.cmd1 = 0x00A0;
                shared.data1 = 0x00A1;
                shared.init_pic([0b00010001, 0b0000_0100, 0x02, 0b0001_0101, 0b0000_0001]);
            }
            Platform::Nec98 => {
                shared.cmd0 = 0x0000;
                shared.data0 = 0x0002;
                shared.cmd1 = 0x0008;
                shared.data1 = 0x000A;
                shared.init_pic([0b00010001, 0b1000_0000, 0x07, 0b0001_1101, 0b0000_1001]);
            }
            Platform::FmTowns => {
                shared.cmd0 = 0x0000;
                shared.data0 = 0x0002;
                shared.cmd1 = 0x0010;
                shared.data1 = 0x0012;
                shared.init_pic([0b00011001, 0b1000_0000, 0x07, 0b0001_1101, 0b0000_1001]);
            }
            _ => unreachable!(),
        }

        let mut irq_table = [0usize; 16];
        asm_handle_irq_table(&mut irq_table);
        for irq in 0..Irq::MAX.0 {
            let offset = irq_table[irq as usize];
            InterruptDescriptorTable::register(
                Irq(irq).into(),
                offset,
                super::cpu::PrivilegeLevel::Kernel,
            );
        }
    }

    #[inline]
    fn shared<'a>() -> &'a mut Self {
        unsafe { &mut PIC }
    }

    #[inline]
    unsafe fn write_cmd0(&self, val: u8) {
        asm!("out dx, al", in("edx") self.cmd0, in("al") val);
    }

    #[inline]
    unsafe fn write_data0(&self, val: u8) {
        asm!("out dx, al", in("edx") self.data0, in("al") val);
    }

    #[inline]
    unsafe fn write_cmd1(&self, val: u8) {
        asm!("out dx, al", in("edx") self.cmd1, in("al") val);
    }

    #[inline]
    unsafe fn write_data1(&self, val: u8) {
        asm!("out dx, al", in("edx") self.data1, in("al") val);
    }

    #[inline]
    unsafe fn read_data0(&self) -> u8 {
        let result: u8;
        asm!("in al, dx", in ("edx") self.data0, lateout ("al") result);
        result
    }

    #[inline]
    unsafe fn read_data1(&self) -> u8 {
        let result: u8;
        asm!("in al, dx", in ("edx") self.data1, lateout ("al") result);
        result
    }

    /// Init PICs
    #[inline]
    unsafe fn init_pic(&mut self, cmds: [u8; 5]) {
        self.write_data0(u8::MAX);
        self.write_data1(u8::MAX);

        self.write_cmd0(cmds[0]);
        self.write_data0(Irq::BASE.0);
        self.write_data0(cmds[1]);
        self.write_data0(cmds[3]);

        self.write_cmd1(cmds[0]);
        self.write_data1(Irq::BASE.0 + 8);
        self.write_data1(cmds[2]);
        self.write_data1(cmds[4]);

        // Enable slave and spurious irq
        self.write_data0(!cmds[1] & 0x7F);
        self.write_data1(0x7F);

        self.chain_eoi = 0x60 + cmds[2];
    }

    pub unsafe fn register(irq: Irq, f: IrqHandler) -> Result<(), ()> {
        Cpu::without_interrupts(|| {
            let shared = Self::shared();
            let irq_index = irq.0 as usize;
            if shared.idt[irq_index] != 0 {
                return Err(());
            }
            shared.idt[irq_index] = f as usize;
            Self::set_irq_enabled(irq, true);
            Ok(())
        })
    }

    pub unsafe fn set_irq_enabled(irq: Irq, enabled: bool) {
        Cpu::without_interrupts(|| {
            let shared = Self::shared();
            if irq.is_slave() {
                let irq = irq.local_number();
                let mut imr = shared.read_data1();
                if enabled {
                    imr &= !(1 << irq);
                } else {
                    imr |= 1 << irq;
                }
                shared.write_data1(imr);
            } else {
                let irq = irq.local_number();
                let mut imr = shared.read_data0();
                if enabled {
                    imr &= !(1 << irq);
                } else {
                    imr |= 1 << irq;
                }
                shared.write_data0(imr);
            }
        })
    }
}

#[no_mangle]
pub unsafe extern "fastcall" fn pic_handle_irq(irq: Irq) {
    asm!("
        mov ds, {0:e}
        mov es, {0:e}
        ", in (reg) Selector::KERNEL_DATA.0);

    let shared = Pic::shared();

    // TODO: irq bound check?
    match NonZeroUsize::new(*shared.idt.get_unchecked(irq.0 as usize)) {
        Some(v) => {
            let f: IrqHandler = core::mem::transmute(v.get());
            f(irq);
        }
        None => (),
    }

    // EOI
    if irq.is_slave() {
        shared.write_cmd1(0x60 + irq.local_number());
        shared.write_cmd0(shared.chain_eoi);
    } else {
        shared.write_cmd0(0x60 + irq.local_number());
    }
}

#[repr(transparent)]
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd, Default)]
pub struct Irq(pub u8);

impl Irq {
    const BASE: InterruptVector = InterruptVector(0x20);
    const MAX: Irq = Irq(16);

    pub const fn as_vec(self) -> InterruptVector {
        InterruptVector(Self::BASE.0 + self.0)
    }

    pub unsafe fn register(&self, f: IrqHandler) -> Result<(), ()> {
        Pic::register(*self, f)
    }

    pub unsafe fn enable(&self) {
        Pic::set_irq_enabled(*self, true)
    }

    pub unsafe fn disable(&self) {
        Pic::set_irq_enabled(*self, false)
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
