// x86

use crate::*;
use bitflags::*;
use core::fmt::Write;

extern "fastcall" {
    fn asm_handle_exception(_: InterruptVector) -> usize;
}

pub struct Cpu {}

impl Cpu {
    pub(crate) unsafe fn init() {
        InterruptDescriptorTable::init();
    }

    #[inline]
    pub fn spin_loop_hint() {
        unsafe { asm!("pause") };
    }

    #[inline]
    pub fn noop() {
        unsafe { asm!("nop") };
    }

    #[inline]
    pub unsafe fn halt() {
        asm!("hlt");
    }

    #[inline]
    pub unsafe fn enable_interrupt() {
        asm!("sti");
    }

    #[inline]
    pub unsafe fn disable_interrupt() {
        asm!("cli");
    }

    #[inline]
    pub(crate) unsafe fn stop() -> ! {
        loop {
            Self::disable_interrupt();
            Self::halt();
        }
    }

    #[inline]
    pub fn breakpoint() {
        unsafe { asm!("int3") };
    }

    pub(crate) unsafe fn reset() -> ! {
        // let _ = MyScheduler::freeze(true);

        // Self::out8(0x0CF9, 0x06);
        // asm!("out 0x92, al", in("al") 0x01 as u8);

        Cpu::stop();
    }

    #[inline]
    pub unsafe fn out8(port: u16, value: u8) {
        asm!("out dx, al", in("dx") port, in("al") value);
    }

    #[inline]
    pub unsafe fn in8(port: u16) -> u8 {
        let mut result: u8;
        asm!("in al, dx", in("dx") port, lateout("al") result);
        result
    }

    #[inline]
    pub unsafe fn out16(port: u16, value: u16) {
        asm!("out dx, ax", in("dx") port, in("ax") value);
    }

    #[inline]
    pub unsafe fn in16(port: u16) -> u16 {
        let mut result: u16;
        asm!("in ax, dx", in("dx") port, lateout("ax") result);
        result
    }

    #[inline]
    pub unsafe fn out32(port: u16, value: u32) {
        asm!("out dx, eax", in("dx") port, in("eax") value);
    }

    #[inline]
    pub unsafe fn in32(port: u16) -> u32 {
        let mut result: u32;
        asm!("in eax, dx", in("dx") port, lateout("eax") result);
        result
    }

    #[allow(dead_code)]
    #[inline]
    #[track_caller]
    pub(crate) fn assert_without_interrupt() {
        let flags = unsafe {
            let eax: u32;
            asm!("
                pushfd
                pop {0}
                ", lateout (reg) eax);
            Eflags::from_bits_unchecked(eax)
        };
        assert!(!flags.contains(Eflags::IF));
    }

    #[inline]
    pub(crate) unsafe fn without_interrupts<F, R>(f: F) -> R
    where
        F: FnOnce() -> R,
    {
        let eax: u32;
        asm!("
            pushfd
            cli
            pop {0}
            ", lateout (reg) eax);
        let flags = Eflags::from_bits_unchecked(eax);

        let result = f();

        if flags.contains(Eflags::IF) {
            Self::enable_interrupt();
        }

        result
    }
}

bitflags! {
    pub struct Eflags: u32 {
        const CF    = 0x0000_0001;
        const PF    = 0x0000_0004;
        const AF    = 0x0000_0010;
        const ZF    = 0x0000_0040;
        const SF    = 0x0000_0080;
        const TF    = 0x0000_0100;
        const IF    = 0x0000_0200;
        const DF    = 0x0000_0400;
        const OF    = 0x0000_0800;
        const IOPL3 = 0x0000_3000;
        const NT    = 0x0000_4000;
        const RF    = 0x0001_0000;
        const VM    = 0x0002_0000;
        const AC    = 0x0004_0000;
        const VIF   = 0x0008_0000;
        const VIP   = 0x0010_0000;
        const ID    = 0x0020_0000;
    }
}

#[repr(transparent)]
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
pub struct Limit(pub u16);

#[repr(transparent)]
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct Selector(pub u16);

impl Selector {
    pub const NULL: Selector = Selector(0);
    pub const SYSTEM_TSS: Selector = Selector::new(1, PrivilegeLevel::Kernel);
    pub const KERNEL_CODE: Selector = Selector::new(2, PrivilegeLevel::Kernel);
    pub const KERNEL_DATA: Selector = Selector::new(3, PrivilegeLevel::Kernel);
    pub const USER_CODE: Selector = Selector::new(4, PrivilegeLevel::User);
    pub const USER_DATA: Selector = Selector::new(5, PrivilegeLevel::User);

    #[inline]
    pub const fn new(index: usize, rpl: PrivilegeLevel) -> Self {
        Selector((index << 3) as u16 | rpl as u16)
    }

    #[inline]
    pub const fn rpl(self) -> PrivilegeLevel {
        PrivilegeLevel::from_usize(self.0 as usize)
    }

    #[inline]
    pub const fn index(self) -> usize {
        (self.0 >> 3) as usize
    }
}

#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
pub enum PrivilegeLevel {
    Kernel = 0,
    Ring1,
    Ring2,
    User,
}

impl PrivilegeLevel {
    pub const fn as_descriptor_entry(self) -> u64 {
        (self as u64) << 45
    }

    pub const fn from_usize(value: usize) -> Self {
        match value & 3 {
            0 => PrivilegeLevel::Kernel,
            1 => PrivilegeLevel::Ring1,
            2 => PrivilegeLevel::Ring2,
            _ => PrivilegeLevel::User,
        }
    }
}

impl From<usize> for PrivilegeLevel {
    fn from(value: usize) -> Self {
        Self::from_usize(value)
    }
}

#[non_exhaustive]
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum DescriptorType {
    Null = 0,
    Tss = 9,
    TssBusy = 11,
    InterruptGate = 14,
    TrapGate = 15,
}

impl DescriptorType {
    pub const fn as_descriptor_entry(self) -> u64 {
        let ty = self as u64;
        ty << 40
    }
}

#[repr(transparent)]
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
pub struct InterruptVector(pub u8);

#[repr(u8)]
#[non_exhaustive]
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum Exception {
    DivideError = 0,
    Debug = 1,
    NonMaskable = 2,
    Breakpoint = 3,
    Overflow = 4,
    //Deprecated = 5,
    InvalidOpcode = 6,
    DeviceNotAvailable = 7,
    DoubleFault = 8,
    //Deprecated = 9,
    InvalidTss = 10,
    SegmentNotPresent = 11,
    StackException = 12,
    GeneralProtection = 13,
    PageFault = 14,
    //Unavailable = 15,
    FloatingPointException = 16,
    AlignmentCheck = 17,
    MachineCheck = 18,
    SimdException = 19,
}

impl Exception {
    pub const fn as_vec(&self) -> InterruptVector {
        InterruptVector(*self as u8)
    }
}

impl From<Exception> for InterruptVector {
    fn from(ex: Exception) -> InterruptVector {
        ex.as_vec()
    }
}

#[repr(C, packed)]
#[derive(Default)]
pub struct TaskStateSegment {
    pub link: u16,
    _reserved_10: u16,
    pub esp0: u32,
    pub ss0: u32,
    pub esp1: u32,
    pub ss1: u32,
    pub esp2: u32,
    pub ss2: u32,
    pub cr3: u32,
    pub eip: u32,
    pub eflags: u32,
    pub eax: u32,
    pub ecx: u32,
    pub edx: u32,
    pub ebx: u32,
    pub esp: u32,
    pub ebp: u32,
    pub esi: u32,
    pub edi: u32,
    pub es: u32,
    pub cs: u32,
    pub ss: u32,
    pub ds: u32,
    pub fs: u32,
    pub gs: u32,
    pub ldtr: u32,
    _reserved_3: u16,
    pub iomap_base: u16,
}

impl TaskStateSegment {
    pub const fn new() -> Self {
        Self {
            link: 0,
            _reserved_10: 0,
            esp0: 0,
            ss0: 0,
            esp1: 0,
            ss1: 0,
            esp2: 0,
            ss2: 0,
            cr3: 0,
            eip: 0,
            eflags: 0,
            eax: 0,
            ecx: 0,
            edx: 0,
            ebx: 0,
            esp: 0,
            ebp: 0,
            esi: 0,
            edi: 0,
            es: 0,
            cs: 0,
            ss: 0,
            ds: 0,
            fs: 0,
            gs: 0,
            ldtr: 0,
            _reserved_3: 0,
            iomap_base: 0,
        }
    }

    #[inline]
    pub const fn limit(&self) -> Limit {
        Limit(0x67)
    }
}

#[repr(u64)]
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum DefaultSize {
    Use16 = 0x0000_0000_0000_0000,
    Use32 = 0x0040_0000_0000_0000,
}

impl DefaultSize {
    #[inline]
    pub const fn as_descriptor_entry(self) -> u64 {
        self as u64
    }
}

#[repr(transparent)]
#[derive(Copy, Clone, PartialEq)]
pub struct DescriptorEntry(pub u64);

impl DescriptorEntry {
    #[inline]
    pub const fn null() -> Self {
        Self(0)
    }

    #[inline]
    pub const fn is_null(self) -> bool {
        self.0 == 0
    }

    #[inline]
    pub const fn present() -> u64 {
        0x8000_0000_0000
    }

    #[inline]
    pub const fn granularity() -> u64 {
        0x0080_0000_0000_0000
    }

    #[inline]
    pub const fn big_data() -> u64 {
        0x0040_0000_0000_0000
    }

    #[inline]
    pub const fn code_segment(
        base: u32,
        limit: u32,
        dpl: PrivilegeLevel,
        size: DefaultSize,
    ) -> DescriptorEntry {
        let limit = if limit > 0xFFFF {
            Self::granularity()
                | ((limit as u64) >> 10) & 0xFFFF
                | ((limit as u64 & 0xF000_0000) << 16)
        } else {
            limit as u64
        };
        DescriptorEntry(
            0x0000_1A00_0000_0000u64
                | limit
                | Self::present()
                | dpl.as_descriptor_entry()
                | size.as_descriptor_entry()
                | ((base as u64 & 0x00FF_FFFF) << 16)
                | ((base as u64 & 0xFF00_0000) << 32),
        )
    }

    #[inline]
    pub const fn data_segment(base: u32, limit: u32, dpl: PrivilegeLevel) -> DescriptorEntry {
        let limit = if limit > 0xFFFF {
            Self::granularity()
                | ((limit as u64) >> 10) & 0xFFFF
                | (limit as u64 & 0xF000_0000) << 16
        } else {
            limit as u64
        };
        DescriptorEntry(
            0x0000_1200_0000_0000u64
                | limit
                | Self::present()
                | Self::big_data()
                | dpl.as_descriptor_entry()
                | ((base as u64 & 0x00FF_FFFF) << 16)
                | ((base as u64 & 0xFF00_0000) << 32),
        )
    }

    #[inline]
    pub const fn tss_descriptor(offset: usize, limit: Limit) -> DescriptorEntry {
        let offset = offset as u64;
        DescriptorEntry(
            limit.0 as u64
                | Self::present()
                | DescriptorType::Tss.as_descriptor_entry()
                | ((offset & 0x00FF_FFFF) << 16)
                | ((offset & 0xFF00_0000) << 32),
        )
    }

    #[inline]
    pub const fn gate_descriptor(
        offset: usize,
        sel: Selector,
        dpl: PrivilegeLevel,
        ty: DescriptorType,
    ) -> DescriptorEntry {
        let offset = offset as u64;
        DescriptorEntry(
            (offset & 0xFFFF)
                | (sel.0 as u64) << 16
                | Self::present()
                | dpl.as_descriptor_entry()
                | ty.as_descriptor_entry()
                | (offset & 0xFFFF_0000) << 32,
        )
    }
}

static mut IDT: InterruptDescriptorTable = InterruptDescriptorTable::new();

#[repr(C, align(16))]
pub struct InterruptDescriptorTable {
    table: [DescriptorEntry; Self::MAX],
}

impl InterruptDescriptorTable {
    const MAX: usize = 256;

    const fn new() -> Self {
        InterruptDescriptorTable {
            table: [DescriptorEntry::null(); Self::MAX],
        }
    }

    unsafe fn init() {
        Self::load();
        for exception in [
            Exception::DivideError,
            Exception::Breakpoint,
            Exception::InvalidOpcode,
            Exception::DoubleFault,
            Exception::GeneralProtection,
            Exception::PageFault,
        ]
        .iter()
        {
            let vec = InterruptVector::from(*exception);
            let offset = asm_handle_exception(vec);
            if offset != 0 {
                Self::register(vec, offset, PrivilegeLevel::Kernel);
            }
        }
    }

    unsafe fn load() {
        asm!("
            push {0}
            push {1}
            lidt [esp+2]
            add esp, 8
            ", in(reg) &IDT.table, in(reg) ((IDT.table.len() * 8 - 1) << 16));
    }

    #[inline]
    pub unsafe fn register(vec: InterruptVector, offset: usize, dpl: PrivilegeLevel) {
        let entry = DescriptorEntry::gate_descriptor(
            offset,
            Selector::KERNEL_CODE,
            dpl,
            if dpl == PrivilegeLevel::Kernel {
                DescriptorType::InterruptGate
            } else {
                DescriptorType::TrapGate
            },
        );
        IDT.table[vec.0 as usize] = entry;
    }
}

#[repr(C)]
pub struct InterruptFrame {
    pub cr2: u32,
    pub edi: u32,
    pub esi: u32,
    pub ebp: u32,
    _esp: u32,
    pub ebx: u32,
    pub edx: u32,
    pub ecx: u32,
    pub eax: u32,
    pub ds: u16,
    _padding_ds: u16,
    pub ss0: u16,
    _padding_ss: u16,
    pub es: u16,
    _padding_es: u16,
    pub intnum: u32,
    pub errcode: u16,
    _padding_err: u16,
    pub eip: u32,
    pub cs: u16,
    _padding_cs: u16,
    pub eflags: u32,
    pub esp3: u32,
    pub ss3: u16,
    _padding_ss3: u16,
    pub vmes: u16,
    _padding_vmes: u16,
    pub vmds: u16,
    _padding_vmds: u16,
    pub vmfs: u16,
    _padding_vmfs: u16,
    pub vmgs: u16,
    _padding_vmgs: u16,
}

#[no_mangle]
pub unsafe extern "C" fn cpu_default_exception(ctx: &mut InterruptFrame) {
    asm!("
        mov ds, {0:e}
        mov es, {0:e}
        ", in(reg) Selector::KERNEL_DATA.0);

    let is_vm = Eflags::from_bits_unchecked(ctx.eflags).contains(Eflags::VM);
    let is_user = is_vm || Selector(ctx.cs).rpl() != PrivilegeLevel::Kernel;

    let ss = if is_user { ctx.ss3 } else { ctx.ss0 };
    let esp = if is_user {
        ctx.esp3
    } else {
        &ctx.esp3 as *const _ as usize as u32
    };
    let ds = if is_vm { ctx.vmds } else { ctx.ds };
    let es = if is_vm { ctx.vmes } else { ctx.es };

    println!("#### EXCEPTION {:02x}-{:04x}", ctx.intnum, ctx.errcode);
    if is_vm {
        println!(
            "CS:IP {:04x}:{:04x} SS:SP {:04x}:{:04x} EFLAGS {:08x}",
            ctx.cs, ctx.eip, ss, esp, ctx.eflags
        );
    } else {
        println!(
            "CS:EIP {:02x}:{:08x} SS:ESP {:02x}:{:08x} EFLAGS {:08x}",
            ctx.cs, ctx.eip, ss, esp, ctx.eflags
        );
    }
    println!(
        "EAX {:08x} EBX {:08x} ECX {:08x} EDX {:08x}",
        ctx.eax, ctx.ebx, ctx.ecx, ctx.edx,
    );
    println!(
        "EBP {:08x} ESI {:08x} EDI {:08x} DS {:04x} ES {:04x}",
        ctx.ebp, ctx.esi, ctx.edi, ds, es
    );

    Cpu::stop();
}
