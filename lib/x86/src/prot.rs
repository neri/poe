//! protected mode structures

use core::convert::TryFrom;
use core::fmt::LowerHex;
use core::mem::transmute;
use paste::paste;

#[repr(transparent)]
#[derive(Copy, Clone, Eq, PartialEq)]
pub struct DescriptorEntry(u64);

impl DescriptorEntry {
    pub const PRESENT: u64 = 0x0000_8000_0000_0000;

    pub const SEGMENT: u64 = 0x0000_1000_0000_0000;

    pub const CODE_OR_DATA: u64 = 0x0000_0800_0000_0000;

    pub const READ_WRITE: u64 = 0x0000_0200_0000_0000;

    pub const BIG_DATA: u64 = 0x0040_0000_0000_0000;

    pub const NULL: Self = Self(0);

    #[inline]
    pub const fn is_null(&self) -> bool {
        (self.0 & 0x0000_1f00_0000_0000) == 0
    }

    #[inline]
    pub const fn is_present(&self) -> bool {
        (self.0 & Self::PRESENT) == Self::PRESENT
    }

    #[inline]
    pub const fn is_segment(&self) -> bool {
        (self.0 & Self::SEGMENT) == Self::SEGMENT
    }

    #[inline]
    pub const fn is_code_segment(&self) -> bool {
        const CODE_SEGMENT: u64 = 0x0000_1800_0000_0000;
        (self.0 & CODE_SEGMENT) == CODE_SEGMENT
    }

    #[inline]
    pub const fn is_data_segment(&self) -> bool {
        const DATA_SEGMENT: u64 = 0x0000_1000_0000_0000;
        (self.0 & DATA_SEGMENT) == DATA_SEGMENT
    }

    #[inline]
    pub const fn code_segment(
        limit: Limit32,
        base: Linear32,
        is_readable: bool,
        dpl: DPL,
        is_present: bool,
        opr_size: DefaultOperandSize,
    ) -> Self {
        Self(
            Self::SEGMENT
                | Self::CODE_OR_DATA
                | limit.as_descriptor_entry()
                | base.as_segment_base()
                | if is_readable { Self::READ_WRITE } else { 0 }
                | dpl.as_descriptor_entry()
                | if is_present { Self::PRESENT } else { 0 }
                | opr_size.as_descriptor_entry(),
        )
    }

    #[inline]
    pub const fn data_segment(
        limit: Limit32,
        base: Linear32,
        is_writable: bool,
        dpl: DPL,
        is_present: bool,
        is_big_data: bool,
    ) -> Self {
        Self(
            Self::SEGMENT
                | limit.as_descriptor_entry()
                | base.as_segment_base()
                | if is_writable { Self::READ_WRITE } else { 0 }
                | dpl.as_descriptor_entry()
                | if is_present { Self::PRESENT } else { 0 }
                | if is_big_data { Self::BIG_DATA } else { 0 },
        )
    }

    #[inline]
    pub const fn tss(limit: Limit16, base: Linear32, is_present: bool) -> Self {
        Self(
            DescriptorType::TSS.as_descriptor_entry()
                | limit.as_descriptor_entry()
                | base.as_segment_base()
                | if is_present { Self::PRESENT } else { 0 },
        )
    }

    #[inline]
    pub const fn gate(
        offset: Offset32,
        sel: Selector,
        ty: DescriptorType,
        dpl: DPL,
        is_present: bool,
    ) -> Self {
        Self(
            offset.as_gate_offset()
                | (sel.as_u16() as u64) << 16
                | ty.as_descriptor_entry()
                | dpl.as_descriptor_entry()
                | if is_present { Self::PRESENT } else { 0 },
        )
    }

    #[cfg(target_arch = "x86_64")]
    #[inline]
    pub const fn gate64(
        offset: Offset64,
        sel: Selector,
        ist: Option<InterruptStackTable>,
        ty: DescriptorType,
        dpl: DPL,
        is_present: bool,
    ) -> DescriptorPair {
        let (offset_low, offset_high) = offset.as_gate_offset_pair();
        let ist = match ist {
            Some(ist) => ist.as_descriptor_entry(),
            None => 0,
        };
        let low = Self(
            offset_low
                | (sel.as_u16() as u64) << 16
                | ist
                | ty.as_descriptor_entry()
                | dpl.as_descriptor_entry()
                | if is_present { Self::PRESENT } else { 0 },
        );
        let high = DescriptorEntry(offset_high);

        DescriptorPair::new(low, high)
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

pub struct SegmentDescriptor;

impl SegmentDescriptor {
    #[inline]
    pub const fn flat_code32(dpl: DPL) -> DescriptorEntry {
        Self::code(Limit32::MAX, Linear32(0), dpl, USE32)
    }

    #[inline]
    pub const fn flat_code64(dpl: DPL) -> DescriptorEntry {
        Self::code(Limit32::MAX, Linear32(0), dpl, USE64)
    }

    #[inline]
    pub const fn flat_data(dpl: DPL) -> DescriptorEntry {
        Self::data(Limit32::MAX, Linear32(0), dpl, true)
    }

    #[inline]
    pub const fn code(
        limit: Limit32,
        base: Linear32,
        dpl: DPL,
        opr_size: DefaultOperandSize,
    ) -> DescriptorEntry {
        DescriptorEntry::code_segment(limit, base, true, dpl, true, opr_size)
    }

    #[inline]
    pub const fn data(
        limit: Limit32,
        base: Linear32,
        dpl: DPL,
        is_big_data: bool,
    ) -> DescriptorEntry {
        DescriptorEntry::data_segment(limit, base, true, dpl, true, is_big_data)
    }

    #[cfg(target_arch = "x86")]
    #[inline]
    pub const fn tss32(limit: Limit16, base: Linear32) -> DescriptorEntry {
        DescriptorEntry::tss(limit, base, true)
    }

    #[cfg(target_arch = "x86_64")]
    #[inline]
    pub const fn tss64(limit: Limit16, base: Linear64) -> DescriptorPair {
        let (base_low, base_high) = base.as_segment_base_pair();
        let low = DescriptorEntry(
            limit.as_descriptor_entry()
                | base_low
                | DescriptorType::TSS.as_descriptor_entry()
                | DescriptorEntry::PRESENT,
        );
        let high = DescriptorEntry(base_high);
        DescriptorPair::new(low, high)
    }
}

pub struct GateDescriptor;

impl GateDescriptor {
    #[cfg(target_arch = "x86")]
    #[inline]
    pub const fn new(
        offset: Offset32,
        sel: Selector,
        ty: DescriptorType,
        dpl: DPL,
    ) -> DescriptorEntry {
        DescriptorEntry::gate(offset, sel, ty, dpl, true)
    }

    #[cfg(target_arch = "x86_64")]
    #[inline]
    pub const fn new(
        offset: Offset64,
        sel: Selector,
        ist: Option<InterruptStackTable>,
        ty: DescriptorType,
        dpl: DPL,
    ) -> DescriptorPair {
        DescriptorEntry::gate64(offset, sel, ist, ty, dpl, true)
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
    pub const MAX: Self = Self(u16::MAX);

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
    pub const ZERO: Self = Self(0);

    pub const MAX: Self = Self(u32::MAX);

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
    pub const ZERO: Self = Self(0);

    pub const MAX: Self = Self(u64::MAX);

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
    pub const ZERO: Self = Self(0);

    pub const MAX: Self = Self(u32::MAX);

    #[inline]
    pub const fn new(val: u32) -> Self {
        Self(val)
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
pub struct Offset64(u64);

impl Offset64 {
    pub const ZERO: Self = Self(0);

    pub const MAX: Self = Self(u64::MAX);

    #[inline]
    pub const fn new(val: u64) -> Self {
        Self(val)
    }

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
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Default)]
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
    pub const fn rpl(&self) -> RPL {
        RPL::from_u16(self.0)
    }

    /// Returns the index field in the selector
    #[inline]
    pub const fn index(&self) -> usize {
        (self.0 >> 3) as usize
    }

    #[inline]
    pub const fn is_global(&self) -> bool {
        (self.0 & Self::TI_LDT) == 0
    }

    #[inline]
    pub const fn is_local(&self) -> bool {
        !self.is_global()
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
pub struct AlignedSelector32(u32);

impl AlignedSelector32 {
    pub const NULL: Self = Self(0);

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
    /// Useless in 64bit mode
    _Ring1 = 1,
    /// Useless in 64bit mode
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

    /// I/O Priviledge Level
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
    pub const fn from_flags(val: Flags) -> IOPL {
        IOPL(PrivilegeLevel::from_usize((val.bits() >> 12) & 3))
    }

    #[inline]
    pub const fn into_flags(self) -> usize {
        (self.0 as usize) << 12
    }
}

#[non_exhaustive]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum DescriptorType {
    NULL = 0,
    LDT = 2,
    TSS = 9,
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
    // Deprecated = 5,
    /// #UD
    InvalidOpcode = 6,
    /// #NM
    DeviceNotAvailable = 7,
    /// #DF
    DoubleFault = 8,
    // Deprecated = 9,
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
    // Unavailable = 15,
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
    //
    // Reserved
    //
    /// #SX
    Security = 30,
    // Reserved = 31,
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
            Exception::ControlProtection => "#CP",
            Exception::MAX => unreachable!(),
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
    pub link: AlignedSelector32,
    pub esp0: Gpr32,
    pub ss0: AlignedSelector32,
    pub esp1: Gpr32,
    pub ss1: AlignedSelector32,
    pub esp2: Gpr32,
    pub ss2: AlignedSelector32,
    pub cr3: u32,
    pub eip: Gpr32,
    pub eflags: u32,
    pub eax: Gpr32,
    pub ecx: Gpr32,
    pub edx: Gpr32,
    pub ebx: Gpr32,
    pub esp: Gpr32,
    pub ebp: Gpr32,
    pub esi: Gpr32,
    pub edi: Gpr32,
    pub es: AlignedSelector32,
    pub cs: AlignedSelector32,
    pub ss: AlignedSelector32,
    pub ds: AlignedSelector32,
    pub fs: AlignedSelector32,
    pub gs: AlignedSelector32,
    pub ldtr: AlignedSelector32,
    /// Debug trap flag
    pub t: u16,
    pub iopb_base: Offset16,
    pub ssp: Gpr32,
}

#[cfg(target_arch = "x86")]
impl TaskStateSegment32 {
    pub const OFFSET_ESP0: usize = 0x04;

    pub const LIMIT: Limit16 = Limit16(0x6b);

    #[inline]
    pub const fn empty() -> Self {
        Self {
            link: AlignedSelector32::NULL,
            esp0: Gpr32::ZERO,
            ss0: AlignedSelector32::NULL,
            esp1: Gpr32::ZERO,
            ss1: AlignedSelector32::NULL,
            esp2: Gpr32::ZERO,
            ss2: AlignedSelector32::NULL,
            cr3: 0,
            eip: Gpr32::ZERO,
            eflags: 0,
            eax: Gpr32::ZERO,
            ecx: Gpr32::ZERO,
            edx: Gpr32::ZERO,
            ebx: Gpr32::ZERO,
            esp: Gpr32::ZERO,
            ebp: Gpr32::ZERO,
            esi: Gpr32::ZERO,
            edi: Gpr32::ZERO,
            es: AlignedSelector32::NULL,
            cs: AlignedSelector32::NULL,
            ss: AlignedSelector32::NULL,
            ds: AlignedSelector32::NULL,
            fs: AlignedSelector32::NULL,
            gs: AlignedSelector32::NULL,
            ldtr: AlignedSelector32::NULL,
            t: 0,
            iopb_base: Offset16::ZERO,
            ssp: Gpr32::ZERO,
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
    _reserved_4: u16,
    pub iopb_base: Offset16,
}

#[cfg(target_arch = "x86_64")]
impl TaskStateSegment64 {
    pub const OFFSET_RSP0: usize = 0x04;

    pub const LIMIT: Limit16 = Limit16(0x67);

    #[inline]
    pub const fn empty() -> Self {
        Self {
            _reserved_1: 0,
            stack_pointer: [0; 3],
            _reserved_2: [0, 0],
            ist: [0; 7],
            _reserved_3: [0, 0],
            _reserved_4: 0,
            iopb_base: Offset16::new(0),
        }
    }

    #[inline]
    pub fn as_descriptor_pair(&self) -> DescriptorPair {
        SegmentDescriptor::tss64(Self::LIMIT, Linear64(self as *const _ as usize as u64))
    }
}

#[repr(u64)]
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum DefaultOperandSize {
    Use16 = 0x0000_0000_0000_0000,
    Use32 = 0x0040_0000_0000_0000,
    Use64 = 0x0020_0000_0000_0000,
}

pub const USE16: DefaultOperandSize = DefaultOperandSize::Use16;

pub const USE32: DefaultOperandSize = DefaultOperandSize::Use32;

pub const USE64: DefaultOperandSize = DefaultOperandSize::Use64;

impl DefaultOperandSize {
    #[inline]
    pub const fn as_descriptor_entry(&self) -> u64 {
        *self as u64
    }

    #[inline]
    pub const fn from_descriptor(value: DescriptorEntry) -> Option<Self> {
        if value.is_code_segment() {
            let is_32 = (value.0 & USE32.as_descriptor_entry()) != 0;
            let is_64 = (value.0 & USE64.as_descriptor_entry()) != 0;
            match (is_32, is_64) {
                (false, false) => Some(USE16),
                (true, false) => Some(USE32),
                (false, true) => Some(USE64),
                _ => None,
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
    pub fn selector(&self) -> Option<Selector> {
        if self.is_idt() {
            None
        } else {
            Some(Selector(self.0 & 0xfffc))
        }
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
        (self.0 & 0b0001) != 0
    }

    #[inline]
    pub fn is_write(&self) -> bool {
        (self.0 & 0b0010) != 0
    }

    #[inline]
    pub fn is_user(&self) -> bool {
        (self.0 & 0b0100) != 0
    }

    #[inline]
    pub fn is_reserved_write(&self) -> bool {
        (self.0 & 0b1000) != 0
    }

    #[inline]
    pub fn is_instruction_fetch(&self) -> bool {
        (self.0 & 0b0001_0000) != 0
    }

    #[inline]
    pub fn is_protection_key(&self) -> bool {
        (self.0 & 0b0010_0000) != 0
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

use crate::gpr::Flags;
#[cfg(target_arch = "x86")]
use crate::gpr::Gpr32;
use crate::real::Offset16;
