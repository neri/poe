//! i386 cpu core logic

use super::vm86::{UserMode, X86StackContextView};
use core::arch::{asm, naked_asm};
use core::cell::UnsafeCell;
use core::mem::size_of;
use core::sync::atomic::{AtomicBool, Ordering, compiler_fence};
use x86::cpuid::{F01C, Feature};
use x86::gpr::Eflags;
use x86::prot::*;

#[cfg(target_arch = "x86")]
pub use core::arch::x86::{__cpuid as cpuid, __cpuid_count as cpuid_count};
#[cfg(target_arch = "x86_64")]
pub use core::arch::x86_64::{__cpuid as cpuid, __cpuid_count as cpuid_count};

#[allow(dead_code)]
static mut SHARED_CPU: UnsafeCell<SharedCpu> = UnsafeCell::new(SharedCpu::new());

/// Shared CPU information
#[allow(dead_code)]
#[derive(Debug)]
pub struct SharedCpu {
    /// The supported instruction set architecture (ISA) level of the CPU, identified during initialization.
    isa_level: IsaLevel,
    /// The maximum supported CPUID leaf for standard function IDs (0x00000000 and below).
    max_cpuid_level_0: u32,
    /// The maximum supported CPUID leaf for extended function IDs (0x80000000 and above).
    max_cpuid_level_8: u32,
    /// The topology mask for simultaneous multithreading (SMT), calculated from CPUID information.
    smt_topology: u32,
    /// Indicates whether simultaneous multithreading (SMT) was detected.
    is_smt_detected: AtomicBool,
    /// Indicates whether hybrid CPU architecture (big.LITTLE) was detected.
    is_hybrid_detected: AtomicBool,
    /// The maximum number of physical address bits supported by the CPU, determined from CPUID information.
    ///
    /// * For 32-bit CPUs, this is typically 36 bits (due to PAE). For 64-bit CPUs, this can be 40 bits or more depending on the specific CPU model and architecture.
    max_physical_address_bits: usize,
    /// The maximum number of virtual address bits supported by the CPU, determined from CPUID information.
    ///
    /// * For 32-bit CPUs, this is typically 32 bits. For 64-bit CPUs, this can be 48 bits or more depending on the specific CPU model and architecture.
    max_virtual_address_bits: usize,
}

pub struct Cpu;

/// Represents the supported instruction set architecture (ISA) levels for the CPU.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum IsaLevel {
    /// Intel 386 or compatible, baseline 32-bit x86 architecture
    I386,
    /// Early Intel 486 or compatible
    I486,
    /// CPU supports the CPUID instruction (later 486, pentium, and later)
    Cpuid,
    /// CPU supports long mode and 64-bit instructions (x86-64-v1)
    X86_64V1,
    /// x86-64-v2
    X86_64V2,
    // /// x86-64-v3
    // X86_64V3,
    // /// x86-64-v4
    // X86_64V4,
}

impl Cpu {
    #[inline]
    pub(crate) unsafe fn init() {
        unsafe {
            let shared = Cpu::shared();
            shared.isa_level = IsaLevel::identify();

            if shared.isa_level >= IsaLevel::Cpuid {
                shared.max_cpuid_level_0 = cpuid(0).eax;
                shared.max_cpuid_level_8 = cpuid(0x8000_0000).eax;

                if shared.max_cpuid_level_0 >= 0x0B {
                    if Feature::HYBRID.exists() {
                        shared.is_hybrid_detected.store(true, Ordering::SeqCst);
                    }
                    if shared.max_cpuid_level_0 >= 0x1F {
                        let cpuid1f = cpuid(0x1F);
                        if (cpuid1f.ecx & 0xFF00) == 0x0100 {
                            shared.smt_topology = (1 << (cpuid1f.eax & 0x1F)) - 1;
                        }
                    } else {
                        let cpuid0b = cpuid(0x0B);
                        if (cpuid0b.ecx & 0xFF00) == 0x0100 {
                            shared.smt_topology = (1 << (cpuid0b.eax & 0x1F)) - 1;
                        }
                    }
                }

                if shared.max_cpuid_level_8 >= 0x8000_0008 {
                    let cpuid88 = cpuid(0x8000_0008);
                    shared.max_physical_address_bits = (cpuid88.eax & 0xFF) as usize;
                    shared.max_virtual_address_bits = ((cpuid88.eax >> 8) & 0xFF) as usize;
                }
            }

            super::gdt32::Gdt::init();
            super::idt::Idt::init();
        }
    }

    #[inline]
    pub unsafe fn shared() -> &'static mut SharedCpu {
        unsafe { (&mut *(&raw mut SHARED_CPU)).get_mut() }
    }

    /// Returns the current CPU's supported instruction set architecture (ISA) level.
    #[inline]
    pub fn isa_level() -> IsaLevel {
        unsafe { Cpu::shared().isa_level }
    }

    /// Jump to user mode with specified stack context
    #[inline(always)]
    pub unsafe fn jump_to_user_mode(regs: &X86StackContextView<UserMode>) -> ! {
        compiler_fence(Ordering::SeqCst);
        unsafe {
            Self::_iret_to_user_mode(regs, super::gdt32::Gdt::shared().tss_mut());
        }
    }

    /// Perform an IRET instruction to return to user mode with the specified stack context.
    #[unsafe(naked)]
    unsafe extern "fastcall" fn _iret_to_user_mode(
        regs: &X86StackContextView<UserMode>,
        tss: &mut TaskStateSegment32,
    ) -> ! {
        naked_asm!(
            "mov [edx + 4], esp",
            "",
            "mov esi, ecx",
            "sub esp, {size_regs}",
            "mov edi, esp",
            "mov ecx, {size_regs} / 4",
            "rep movsd",
            "",
            ".byte 0x0f, 0xa9", // pop gs
            ".byte 0x0f, 0xa1", // pop fs
            ".byte 0x1f", // pop ds
            ".byte 0x07", // pop es
            "",
            "popad",
            "add esp, 8",
            "",
            "iretd",
            size_regs = const size_of::<X86StackContextView<UserMode>>(),
        );
    }

    /// Fill memory with zeros using `rep stosd`.
    #[inline(always)]
    pub unsafe fn zero_memory32(dst: *mut u32, count: usize) -> *mut u32 {
        unsafe { Self::rep_stosd(dst, 0, count) }
    }

    /// Fill memory with a 32-bit value using `rep stosd`.
    ///
    /// Returns the destination pointer after filling.
    ///
    /// # Safety
    ///
    /// * The DF flag must be cleared before calling this function. (normally, it should be cleared by default)
    /// * Memory range safety must be guaranteed by the caller.
    #[inline(always)]
    pub unsafe fn rep_stosd(dst: *mut u32, value: u32, count: usize) -> *mut u32 {
        let mut result;
        unsafe {
            asm!(
                "rep stosd",
                inout("edi") dst => result,
                in("eax") value,
                inout("ecx") count => _,
            );
        }
        result
    }

    /// Copy memory from `src` to `dst` using `rep movsd`.
    ///
    /// Returns the destination pointer and source pointer after copying.
    ///
    /// # Safety
    ///
    /// * The DF flag must be cleared before calling this function. (normally, it should be cleared by default)
    /// * Memory range safety must be guaranteed by the caller.
    #[inline(always)]
    pub unsafe fn rep_movsd(
        dst: *mut u32,
        src: *const u32,
        count: usize,
    ) -> (*mut u32, *const u32) {
        let (mut edi, mut esi) = (dst, src);
        unsafe {
            asm!(
                "xchg esi, {0}",
                "rep movsd",
                "xchg esi, {0}",
                inout(reg) esi,
                inout("edi") edi,
                inout("ecx") count => _,
            );
        }
        (edi, esi)
    }
}

impl SharedCpu {
    #[inline]
    const fn new() -> Self {
        Self {
            isa_level: IsaLevel::I386,
            max_cpuid_level_0: 0,
            max_cpuid_level_8: 0,
            smt_topology: 0,
            is_smt_detected: AtomicBool::new(false),
            is_hybrid_detected: AtomicBool::new(false),
            max_physical_address_bits: 32,
            max_virtual_address_bits: 32,
        }
    }
}

impl IsaLevel {
    /// Identify the CPU's supported instruction set architecture (ISA) level.
    pub fn identify() -> IsaLevel {
        unsafe {
            if cfg!(target_arch = "x86") {
                // check 486 or later by testing if AC flag can be set in EFLAGS
                let result: usize;
                asm!(
                    "push {0}",
                    "popfd",
                    "pushfd",
                    "pop {1}",
                    "and {0}, {1}",
                    inlateout(reg) Eflags::AC.bits() => result,
                    lateout(reg) _,
                );
                if result == 0 {
                    // AC flag cannot be set, so it's an i386 CPU
                    return IsaLevel::I386;
                }

                // check CPUID support by toggling ID flag in EFLAGS
                let result: usize;
                asm!(
                    "pushfd",
                    "pop {0}",
                    "mov {1}, {0}",
                    "xor {1}, {id}",
                    "push {1}",
                    "popfd",
                    "pushfd",
                    "pop {0}",
                    "xor {0}, {1}",
                    lateout(reg) result,
                    lateout(reg) _,
                    id = const Eflags::ID.bits(),
                );
                if result != 0 {
                    // ID flag cannot be toggled, so it's a 486 CPU without CPUID support
                    return IsaLevel::I486;
                }

                // check long mode support by checking if CPUID leaf 0x80000001 is supported and if it has the LM bit set
                if !Feature::LM.exists() {
                    // CPUID is supported, but long mode is not supported, so it's a 32-bit CPU with CPUID support
                    return IsaLevel::Cpuid;
                }
            }

            // check x86-64-v2 support by checking for the presence of certain features that are required for x86-64-v2 support
            let ecx = cpuid(1).ecx;
            let v2_ecx = (1u32 << F01C::SSE3 as usize)
                | (1u32 << F01C::SSSE3 as usize)
                | (1u32 << F01C::CX16 as usize)
                | (1u32 << F01C::SSE4_1 as usize)
                | (1u32 << F01C::SSE4_2 as usize)
                | (1u32 << F01C::POPCNT as usize);
            if (ecx & v2_ecx) != v2_ecx {
                // Not all x86-64-v2 features are supported, so it's an x86-64-v1 CPU
                return IsaLevel::X86_64V1;
            }

            IsaLevel::X86_64V2
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SetDescriptorError {
    OutOfIndex,
    PriviledgeMismatch,
}
