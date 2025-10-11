//! protected mode structures

use core::{convert::TryFrom, fmt::LowerHex, mem::transmute};
use paste::paste;

#[allow(unused_imports)]
use core::arch::asm;

#[repr(transparent)]
#[derive(Copy, Clone, Eq, PartialEq)]
pub struct DescriptorEntry(u64);

impl DescriptorEntry {
    pub const PRESENT: u64 = 0x8000_0000_0000;

    pub const BIG_DATA: u64 = 0x0040_0000_0000_0000;

    pub const NULL: Self = Self(0);

    #[inline]
    pub const fn flat_code_segment(dpl: DPL, opr_size: DefaultOperandSize) -> DescriptorEntry {
        Self::code_segment(Linear32(0), Limit32::MAX, dpl, opr_size)
    }

    #[inline]
    pub const fn code_segment(
        base: Linear32,
        limit: Limit32,
        dpl: DPL,
        opr_size: DefaultOperandSize,
    ) -> DescriptorEntry {
        DescriptorEntry(
            0x0000_1A00_0000_0000u64
                | base.as_segment_base()
                | limit.as_descriptor_entry()
                | Self::PRESENT
                | dpl.as_descriptor_entry()
                | opr_size.as_descriptor_entry(),
        )
    }

    #[inline]
    pub const fn flat_data_segment(dpl: DPL) -> DescriptorEntry {
        Self::data_segment(Linear32(0), Limit32::MAX, dpl, true)
    }

    #[inline]
    pub const fn data_segment(
        base: Linear32,
        limit: Limit32,
        dpl: DPL,
        is_big_data: bool,
    ) -> DescriptorEntry {
        DescriptorEntry(
            0x0000_1200_0000_0000u64
                | base.as_segment_base()
                | limit.as_descriptor_entry()
                | Self::PRESENT
                | if is_big_data { Self::BIG_DATA } else { 0 }
                | dpl.as_descriptor_entry(),
        )
    }

    #[cfg(target_arch = "x86")]
    #[inline]
    pub const fn tss32(base: Linear32, limit: Limit16) -> DescriptorEntry {
        DescriptorEntry(
            DescriptorType::Tss.as_descriptor_entry()
                | base.as_segment_base()
                | limit.as_descriptor_entry()
                | Self::PRESENT,
        )
    }

    #[cfg(target_arch = "x86_64")]
    #[inline]
    pub const fn tss64(base: Linear64, limit: Limit16) -> DescriptorPair {
        let (base_low, base_high) = base.as_segment_base_pair();
        let low = DescriptorEntry(
            DescriptorType::Tss.as_descriptor_entry()
                | base_low
                | limit.as_descriptor_entry()
                | Self::PRESENT,
        );
        let high = DescriptorEntry(base_high);
        DescriptorPair::new(low, high)
    }

    #[cfg(target_arch = "x86")]
    #[inline]
    pub const fn gate32(
        offset: usize,
        sel: Selector,
        dpl: DPL,
        ty: DescriptorType,
    ) -> DescriptorEntry {
        let offset = offset as u64;
        DescriptorEntry(
            (offset & 0xFFFF)
                | (sel.0 as u64) << 16
                | dpl.as_descriptor_entry()
                | ty.as_descriptor_entry()
                | (offset & 0xFFFF_0000) << 32
                | Self::PRESENT,
        )
    }

    #[cfg(target_arch = "x86_64")]
    #[inline]
    pub const fn gate64(
        offset: Offset64,
        sel: Selector,
        dpl: DPL,
        ty: DescriptorType,
        ist: Option<InterruptStackTable>,
    ) -> DescriptorPair {
        let (offset_low, offset_high) = offset.as_gate_offset_pair();
        let ist = match ist {
            Some(ist) => ist.as_descriptor_entry(),
            None => 0,
        };
        let low = DescriptorEntry(
            ty.as_descriptor_entry()
                | offset_low
                | sel.as_descriptor_entry()
                | ist
                | dpl.as_descriptor_entry()
                | Self::PRESENT,
        );
        let high = DescriptorEntry(offset_high);

        DescriptorPair::new(low, high)
    }

    #[inline]
    pub const fn is_null(&self) -> bool {
        (self.0 & 0x1f00_0000_0000) == 0
    }

    #[inline]
    pub const fn is_present(&self) -> bool {
        (self.0 & Self::PRESENT) != 0
    }

    #[inline]
    pub const fn is_segment(&self) -> bool {
        (self.0 & 0x1000_0000_0000) != 0
    }

    #[inline]
    pub const fn is_code_segment(&self) -> bool {
        self.is_segment() && (self.0 & 0x0800_0000_0000) != 0
    }

    #[inline]
    pub const fn default_operand_size(&self) -> Option<DefaultOperandSize> {
        DefaultOperandSize::from_descriptor(*self)
    }

    #[inline]
    pub const fn dpl(&self) -> DPL {
        DPL::from_descriptor_entry(self.0)
    }
}

#[repr(C)]
#[derive(Copy, Clone, PartialEq)]
pub struct DescriptorPair {
    pub low: DescriptorEntry,
    pub high: DescriptorEntry,
}

impl DescriptorPair {
    #[inline]
    pub const fn new(low: DescriptorEntry, high: DescriptorEntry) -> Self {
        DescriptorPair { low, high }
    }
}

/// Type of x86 Segment Limit
#[repr(transparent)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Limit16(u16);

impl Limit16 {
    #[inline]
    pub const fn as_descriptor_entry(&self) -> u64 {
        self.0 as u64
    }

    #[inline]
    pub const fn new(val: u16) -> Self {
        Limit16(val)
    }

    #[inline]
    pub const fn as_u16(&self) -> u16 {
        self.0
    }

    #[inline]
    pub const fn as_u32(&self) -> u32 {
        self.0 as u32
    }
}

/// Type of x86 Segment Limit
#[repr(transparent)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Limit32(u32);

impl Limit32 {
    pub const MAX: Self = Self(u32::MAX);

    #[inline]
    pub const fn as_descriptor_entry(&self) -> u64 {
        let limit = self.0;
        if limit > 0xFFFF {
            0x0080_0000_0000_0000
                | ((limit as u64) >> 12) & 0xFFFF
                | ((limit as u64 & 0xF000_0000) << 20)
        } else {
            limit as u64
        }
    }

    #[inline]
    pub const fn new(val: u32) -> Self {
        Limit32(val)
    }

    #[inline]
    pub const fn as_u32(&self) -> u32 {
        self.0
    }
}

/// Type of 32bit Linear Address
#[repr(transparent)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Linear32(u32);

impl Linear32 {
    pub const NULL: Linear32 = Linear32(0);

    #[inline]
    pub const fn new(val: u32) -> Self {
        Linear32(val)
    }

    #[inline]
    pub const fn as_u32(&self) -> u32 {
        self.0
    }

    #[inline]
    pub const fn as_ptr<T: Sized>(&self) -> *mut T {
        self.0 as *mut T
    }

    #[inline]
    pub const fn as_segment_base(&self) -> u64 {
        ((self.0 as u64 & 0x00FF_FFFF) << 16) | ((self.0 as u64 & 0xFF00_0000) << 32)
    }
}

/// Type of 64bit Linear Address
#[repr(transparent)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Linear64(u64);

impl Linear64 {
    #[inline]
    pub const fn as_u64(&self) -> u64 {
        self.0
    }

    #[inline]
    pub const fn as_ptr<T: Sized>(&self) -> *mut T {
        self.0 as *mut T
    }

    #[inline]
    pub const fn as_segment_base_pair(&self) -> (u64, u64) {
        let low = Linear32(self.0 as u32).as_segment_base();
        let high = self.0 >> 32;
        (low, high)
    }
}

/// Type of 32bit Offset Address
#[repr(transparent)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Offset32(u32);

impl Offset32 {
    #[inline]
    pub const fn new(off: u32) -> Self {
        Self(off)
    }

    #[inline]
    pub const fn as_u32(&self) -> u32 {
        self.0
    }

    #[inline]
    pub const fn as_gate_offset(&self) -> u64 {
        let offset = self.0 as u64;
        (offset & 0xFFFF) | (offset & 0xFFFF_0000) << 32
    }
}

/// Type of 64bit Offset Address
#[repr(transparent)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Offset64(pub u64);

impl Offset64 {
    #[inline]
    pub const fn as_u64(&self) -> u64 {
        self.0
    }

    #[inline]
    pub const fn as_gate_offset_pair(&self) -> (u64, u64) {
        let low = Offset32(self.0 as u32).as_gate_offset();
        let high = self.0 >> 32;
        (low, high)
    }
}

/// Type of x86 Segment Selector
#[repr(transparent)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Selector(pub u16);

impl Selector {
    /// The NULL selector that does not contain anything
    pub const NULL: Selector = Selector(0);

    /// Indicates that this selector is an LDT selector
    const TI_LDT: u16 = 0x0004;

    /// Make a new selector from the specified index and RPL
    #[inline]
    pub const fn new(index: u16, rpl: RPL) -> Self {
        Selector((index << 3) | rpl.as_u16())
    }

    /// Make a new LDT selector from the specified index and RPL
    #[inline]
    pub const fn new_local(index: u16, rpl: RPL) -> Self {
        Selector((index << 3) | rpl.as_u16() | Self::TI_LDT)
    }

    /// Returns the requested privilege level in the selector
    #[inline]
    pub const fn rpl(self) -> RPL {
        RPL::from_u16(self.0)
    }

    /// Adjust RPL Field
    #[cfg(target_arch = "x86")]
    #[inline]
    pub fn adjust_rpl(self, rhs: RPL) -> Result<Selector, Selector> {
        let result: u16;
        let setnz: u8;
        unsafe {
            asm!(
                "arpl {0:x}, {1:x}",
                "setnz {2}",
                inout(reg) self.as_u16() => result,
                in(reg) rhs.0 as u16,
                lateout(reg_byte) setnz,
            );
        }
        if setnz == 0 {
            return Ok(Selector(result));
        } else {
            return Err(self);
        }
    }

    /// Returns the index field in the selector
    #[inline]
    pub const fn index(self) -> usize {
        (self.0 >> 3) as usize
    }

    #[inline]
    pub const fn is_global(self) -> bool {
        !self.is_local()
    }

    #[inline]
    pub const fn is_local(self) -> bool {
        (self.0 & Self::TI_LDT) == Self::TI_LDT
    }

    #[inline]
    pub const fn as_u16(&self) -> u16 {
        self.0
    }

    #[inline]
    pub const fn as_usize(&self) -> usize {
        self.0 as usize
    }

    #[inline]
    pub const fn as_descriptor_entry(&self) -> u64 {
        (self.0 as u64) << 16
    }
}

impl LowerHex for Selector {
    #[inline]
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        self.0.fmt(f)
    }
}

/// 32-bit aligned selector
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AlignedSelector32(pub u32);

impl AlignedSelector32 {
    #[inline]
    pub const fn sel(&self) -> Selector {
        Selector(self.0 as u16)
    }
}

impl From<Selector> for AlignedSelector32 {
    #[inline]
    fn from(value: Selector) -> Self {
        Self(value.as_u16() as u32)
    }
}

/// DPL, CPL, RPL and IOPL
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum PrivilegeLevel {
    /// Ring 0, Supervisor mode
    Supervisor = 0,
    /// Historical, Useless in 64bit mode
    _Ring1 = 1,
    /// Historical, Useless in 64bit mode
    _Ring2 = 2,
    /// Ring 3, User mode
    User = 3,
}

impl PrivilegeLevel {
    #[inline]
    pub const fn from_usize(value: usize) -> Self {
        match value & 3 {
            0 => PrivilegeLevel::Supervisor,
            1 => PrivilegeLevel::_Ring1,
            2 => PrivilegeLevel::_Ring2,
            3 => PrivilegeLevel::User,
            _ => unreachable!(),
        }
    }
}

macro_rules! privilege_level_impl {
    ($( $(#[$meta:meta])* $vis:vis struct $class:ident ; )+) => {
        $(
            #[repr(transparent)]
            #[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
            $(#[$meta])*
            $vis struct $class(PrivilegeLevel);

            impl $class {
                $vis const SUPERVISOR: Self = Self(PrivilegeLevel::Supervisor);

                $vis const USER: Self = Self(PrivilegeLevel::User);

                #[inline]
                pub const fn eq(&self, rhs: &Self) -> bool {
                    self.0 as usize == rhs.0 as usize
                }

                #[inline]
                pub const fn ne(&self, rhs: &Self) -> bool {
                    self.0 as usize != rhs.0 as usize
                }
            }

            paste! {
                $vis const [<$class 0>]: $class = $class::SUPERVISOR;

                $vis const [<$class 3>]: $class = $class::USER;
            }

            impl From<PrivilegeLevel> for $class {
                #[inline]
                fn from(val: PrivilegeLevel) -> Self {
                    Self(val)
                }
            }

            impl From<$class> for PrivilegeLevel {
                #[inline]
                fn from(val: $class) -> Self {
                    val.0
                }
            }
        )*
    };
}

privilege_level_impl! {
    /// Current Priviledge Level
    pub struct CPL;

    /// Descriptor Priviledge Level
    pub struct DPL;

    /// Requested Priviledge Level
    pub struct RPL;

    /// I/O Priviledge Level (Historical use only)
    pub struct IOPL;
}

impl DPL {
    #[inline]
    pub const fn as_descriptor_entry(&self) -> u64 {
        (self.0 as u64) << 45
    }

    #[inline]
    pub const fn from_descriptor_entry(val: u64) -> Self {
        Self(PrivilegeLevel::from_usize((val >> 45) as usize))
    }

    #[inline]
    pub const fn as_rpl(self) -> RPL {
        RPL(self.0)
    }
}

impl RPL {
    #[inline]
    pub const fn from_u16(val: u16) -> Self {
        Self(PrivilegeLevel::from_usize(val as usize))
    }

    #[inline]
    pub const fn as_u16(self) -> u16 {
        self.0 as u16
    }
}

impl IOPL {
    #[inline]
    pub const fn from_flags(val: usize) -> IOPL {
        IOPL(PrivilegeLevel::from_usize(val >> 12))
    }

    #[inline]
    pub const fn into_flags(self) -> usize {
        (self.0 as usize) << 12
    }
}

#[non_exhaustive]
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum DescriptorType {
    Null = 0,
    Tss = 9,
    TssBusy = 11,
    InterruptGate = 14,
    TrapGate = 15,
}

impl DescriptorType {
    #[inline]
    pub const fn as_descriptor_entry(&self) -> u64 {
        let ty = *self as u64;
        ty << 40
    }
}

#[repr(transparent)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct InterruptVector(pub u8);

#[repr(u8)]
#[non_exhaustive]
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Exception {
    /// #DE
    DivideError = 0,
    /// #DB
    Debug = 1,
    /// NMI
    NonMaskable = 2,
    /// #BP
    Breakpoint = 3,
    /// #OF
    Overflow = 4,
    //Deprecated = 5,
    /// #UD
    InvalidOpcode = 6,
    /// #NM
    DeviceNotAvailable = 7,
    /// #DF
    DoubleFault = 8,
    //Deprecated = 9,
    /// #TS
    InvalidTss = 10,
    /// #NP
    SegmentNotPresent = 11,
    /// #SS
    StackException = 12,
    /// #GP
    GeneralProtection = 13,
    /// #PF
    PageFault = 14,
    //Unavailable = 15,
    /// #MF
    FloatingPointException = 16,
    /// #AC
    AlignmentCheck = 17,
    /// #MC
    MachineCheck = 18,
    /// #XM
    SimdException = 19,
    /// #VE
    Virtualization = 20,
    /// #CP
    ControlProtection = 21,
    //Reserved
    /// #SX
    Security = 30,
    //Reserved = 31,
    MAX = 32,
}

impl Exception {
    #[inline]
    pub const fn as_vec(self) -> InterruptVector {
        InterruptVector(self as u8)
    }

    #[inline]
    pub fn try_from_vec(vec: InterruptVector) -> Result<Exception, u8> {
        match vec.0 {
            0 => Ok(Exception::DivideError),
            1 => Ok(Exception::Debug),
            2 => Ok(Exception::NonMaskable),
            3 => Ok(Exception::Breakpoint),
            4 => Ok(Exception::Overflow),
            6 => Ok(Exception::InvalidOpcode),
            7 => Ok(Exception::DeviceNotAvailable),
            8 => Ok(Exception::DoubleFault),
            10 => Ok(Exception::InvalidTss),
            11 => Ok(Exception::SegmentNotPresent),
            12 => Ok(Exception::StackException),
            13 => Ok(Exception::GeneralProtection),
            14 => Ok(Exception::PageFault),
            16 => Ok(Exception::FloatingPointException),
            17 => Ok(Exception::AlignmentCheck),
            18 => Ok(Exception::MachineCheck),
            19 => Ok(Exception::SimdException),
            20 => Ok(Exception::Virtualization),
            21 => Ok(Exception::ControlProtection),
            30 => Ok(Exception::Security),
            raw => Err(raw), // Reserved or Unavailable
        }
    }

    /// # Safety
    ///
    /// UB on invalid value.
    #[inline]
    pub const unsafe fn from_vec_unchecked(vec: InterruptVector) -> Self {
        unsafe { transmute(vec.0) }
    }

    #[inline]
    pub const fn has_error_code(&self) -> bool {
        match self {
            Exception::DoubleFault
            | Exception::InvalidTss
            | Exception::SegmentNotPresent
            | Exception::StackException
            | Exception::GeneralProtection
            | Exception::PageFault
            | Exception::AlignmentCheck
            | Exception::Security => true,
            _ => false,
        }
    }

    #[inline]
    pub const fn mnemonic(&self) -> &'static str {
        match self {
            Exception::DivideError => "#DE",
            Exception::Debug => "#DB",
            Exception::NonMaskable => "NMI",
            Exception::Breakpoint => "#BP",
            Exception::Overflow => "#OV",
            Exception::InvalidOpcode => "#UD",
            Exception::DeviceNotAvailable => "#NM",
            Exception::DoubleFault => "#DF",
            Exception::InvalidTss => "#TS",
            Exception::SegmentNotPresent => "#NP",
            Exception::StackException => "#SS",
            Exception::GeneralProtection => "#GP",
            Exception::PageFault => "#PF",
            Exception::FloatingPointException => "#MF",
            Exception::AlignmentCheck => "#AC",
            Exception::MachineCheck => "#MC",
            Exception::SimdException => "#XM",
            Exception::Virtualization => "#VE",
            Exception::Security => "#SX",
            _ => "",
        }
    }
}

impl From<Exception> for InterruptVector {
    #[inline]
    fn from(ex: Exception) -> Self {
        InterruptVector(ex as u8)
    }
}

impl TryFrom<InterruptVector> for Exception {
    type Error = u8;
    #[inline]
    fn try_from(vec: InterruptVector) -> Result<Self, Self::Error> {
        Exception::try_from_vec(vec)
    }
}

#[cfg(target_arch = "x86")]
#[repr(C, packed)]
#[derive(Default)]
pub struct TaskStateSegment32 {
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
    pub iopb_base: u16,
}

#[cfg(target_arch = "x86")]
impl TaskStateSegment32 {
    pub const OFFSET_ESP0: usize = 0x04;

    pub const LIMIT: u16 = 0x67;

    #[inline]
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
            iopb_base: 0,
        }
    }
}

#[cfg(target_arch = "x86_64")]
#[repr(C, packed)]
#[derive(Default)]
pub struct TaskStateSegment64 {
    _reserved_1: u32,
    pub stack_pointer: [u64; 3],
    _reserved_2: [u32; 2],
    pub ist: [u64; 7],
    _reserved_3: [u32; 2],
    pub iopb_base: u16,
}

#[cfg(target_arch = "x86_64")]
impl TaskStateSegment64 {
    pub const OFFSET_RSP0: usize = 0x04;

    pub const LIMIT: u16 = 0x67;

    #[inline]
    pub const fn new() -> Self {
        Self {
            _reserved_1: 0,
            stack_pointer: [0; 3],
            _reserved_2: [0, 0],
            ist: [0; 7],
            _reserved_3: [0, 0],
            iopb_base: 0,
        }
    }

    #[inline]
    pub fn as_descriptor_pair(&self) -> DescriptorPair {
        DescriptorEntry::tss64(
            Linear64(self as *const _ as usize as u64),
            Limit16(Self::LIMIT),
        )
    }
}

#[repr(u64)]
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum DefaultOperandSize {
    Use16 = 0x0000_0000_0000_0000,
    Use32 = 0x0040_0000_0000_0000,

    #[cfg(target_arch = "x86")]
    _Use64 = 0x0020_0000_0000_0000,
    #[cfg(target_arch = "x86_64")]
    Use64 = 0x0020_0000_0000_0000,
}

pub const USE16: DefaultOperandSize = DefaultOperandSize::Use16;

pub const USE32: DefaultOperandSize = DefaultOperandSize::Use32;

#[cfg(target_arch = "x86_64")]
pub const USE64: DefaultOperandSize = DefaultOperandSize::Use64;

impl DefaultOperandSize {
    #[inline]
    pub const fn as_descriptor_entry(&self) -> u64 {
        *self as u64
    }

    #[inline]
    pub const fn from_descriptor(value: DescriptorEntry) -> Option<Self> {
        if value.is_code_segment() {
            #[cfg(target_arch = "x86")]
            {
                let is_32 = (value.0 & USE32.as_descriptor_entry()) != 0;
                let is_64 = (value.0 & DefaultOperandSize::_Use64.as_descriptor_entry()) != 0;
                match (is_32, is_64) {
                    (false, false) => Some(USE16),
                    (true, false) => Some(USE32),
                    (_, true) => None,
                }
            }

            #[cfg(target_arch = "x86_64")]
            {
                let is_32 = (value.0 & USE32.as_descriptor_entry()) != 0;
                let is_64 = (value.0 & USE64.as_descriptor_entry()) != 0;
                match (is_32, is_64) {
                    (false, false) => Some(USE16),
                    (false, true) => Some(USE64),
                    (true, false) => Some(USE32),
                    (true, true) => None,
                }
            }
        } else {
            None
        }
    }
}

impl TryFrom<DescriptorEntry> for DefaultOperandSize {
    type Error = ();
    fn try_from(value: DescriptorEntry) -> Result<Self, Self::Error> {
        Self::from_descriptor(value).ok_or(())
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct SelectorErrorCode(pub u16);

impl SelectorErrorCode {
    #[inline]
    pub fn is_external(&self) -> bool {
        (self.0 & 0b001) != 0
    }

    #[inline]
    pub fn is_idt(&self) -> bool {
        (self.0 & 0b010) == 0b010
    }

    #[inline]
    pub fn is_gdt(&self) -> bool {
        (self.0 & 0b110) == 0b000
    }

    #[inline]
    pub fn is_ldt(&self) -> bool {
        (self.0 & 0b110) == 0b100
    }

    #[inline]
    pub fn index(&self) -> u16 {
        self.0 >> 3
    }

    #[inline]
    pub fn int_vec(&self) -> Option<InterruptVector> {
        if self.is_idt() {
            Some(InterruptVector(self.index() as u8))
        } else {
            None
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct PageErrorCode(pub usize);

impl PageErrorCode {
    #[inline]
    pub fn is_present(&self) -> bool {
        (self.0 & 0x1) != 0
    }

    #[inline]
    pub fn is_write(&self) -> bool {
        (self.0 & 0x2) != 0
    }

    #[inline]
    pub fn is_user(&self) -> bool {
        (self.0 & 0x4) != 0
    }

    #[inline]
    pub fn is_reserved_write(&self) -> bool {
        (self.0 & 0x8) != 0
    }

    #[inline]
    pub fn is_instruction_fetch(&self) -> bool {
        (self.0 & 0x10) != 0
    }

    #[inline]
    pub fn is_protection_key(&self) -> bool {
        (self.0 & 0x20) != 0
    }
}

#[cfg(target_arch = "x86_64")]
mod ist {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
    pub enum InterruptStackTable {
        IST1 = 1,
        IST2,
        IST3,
        IST4,
        IST5,
        IST6,
        IST7,
    }

    macro_rules! ist_impl {
        ($( $ist:ident , )*) => {
            $(
                pub const $ist: InterruptStackTable = InterruptStackTable::$ist;
            )*
        };
    }

    ist_impl!(IST1, IST2, IST3, IST4, IST5, IST6, IST7,);

    impl InterruptStackTable {
        #[inline]
        pub const fn as_descriptor_entry(&self) -> u64 {
            (*self as u64) << 32
        }
    }
}

#[cfg(target_arch = "x86_64")]
pub use ist::*;
