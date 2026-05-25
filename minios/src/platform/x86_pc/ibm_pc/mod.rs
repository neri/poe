//! IBM PC compatible architecture specific code

mod cga_text;
mod disk_bios;
mod ps2;
mod uart;
mod vesa_bios;

#[allow(unused)]
mod bios {
    use crate::arch::vm86::BiosCallVector;

    /// Video BIOS Services
    pub const INT10: BiosCallVector<0x10> = BiosCallVector::new();

    /// Disk BIOS Services
    pub const INT13: BiosCallVector<0x13> = BiosCallVector::new();

    /// Misc BIOS Services
    pub const INT15: BiosCallVector<0x15> = BiosCallVector::new();

    /// Keyboard BIOS Services
    pub const INT16: BiosCallVector<0x16> = BiosCallVector::new();
}

use core::ffi::c_void;
use core::iter::Iterator;
use core::ops::Range;

use acpi::{ACPI_10_TABLE_GUID, ACPI_20_TABLE_GUID, RsdPtr, RsdPtrV1};
use smbios::{SMBIOS_GUID, SmBios};
use x86::gpr::Eflags;
use x86::isolated_io::{IoPortWB, LoIoPortRB, LoIoPortWB};

use super::pic::Irq;
use crate::arch::lomem::LoMemoryManager;
use crate::arch::vm86::Vm86Context;
use crate::mem::{MemoryManager, MemoryType};
use crate::*;

const USE_UART_STDIO: bool = false;

pub(super) unsafe fn init(_info: &SsblInfo) {
    unsafe {
        if USE_UART_STDIO {
            uart::Uart16550::init((0x400 as *const u16).read_volatile());
            let stdout = uart::Uart16550::shared();
            System::set_stdout(stdout);
            let stdin = uart::Uart16550::shared();
            System::set_stdin(stdin);
        } else {
            cga_text::CgaText::init();
        }

        let ebda = ((0x40e as *const u16).read_volatile() as u32) << 4;

        // find ACPI RSD Ptr tables
        {
            let mut acpi1 = None;
            let mut acpi2 = None;

            if ebda > 0 {
                for i in (0..0x400).step_by(16) {
                    if acpi1.is_some() && acpi2.is_some() {
                        break;
                    }
                    let p = (ebda + i) as *const c_void;
                    if RsdPtr::parse_extended(p).is_some() {
                        acpi2 = NonNullPhysicalAddress::from_ptr(p)
                    } else if RsdPtrV1::parse(p).is_some() {
                        acpi1 = NonNullPhysicalAddress::from_ptr(p)
                    }
                }
            }

            for i in (0xe0000..0xfffff).step_by(16) {
                if acpi1.is_some() && acpi2.is_some() {
                    break;
                }
                let p = i as *const c_void;
                if RsdPtr::parse_extended(p).is_some() {
                    acpi2 = NonNullPhysicalAddress::from_ptr(p)
                } else if RsdPtrV1::parse(p).is_some() {
                    acpi1 = NonNullPhysicalAddress::from_ptr(p)
                }
            }

            if let Some(acpi1) = acpi1 {
                System::add_config_table_entry(&ACPI_10_TABLE_GUID, acpi1);
            }
            if let Some(acpi2) = acpi2 {
                System::add_config_table_entry(&ACPI_20_TABLE_GUID, acpi2);
            }
        }

        // find SMBIOS entry
        {
            let mut smbios = None;

            for i in (0xf0000..0xfffff).step_by(16) {
                if smbios.is_some() {
                    break;
                }
                let p = i as *const c_void;
                if SmBios::parse(p).is_some() {
                    smbios = NonNullPhysicalAddress::from_ptr(p);
                }
            }
            if let Some(smbios) = smbios {
                System::add_config_table_entry(&SMBIOS_GUID, smbios);
            }
        }

        arch::vm86::VM86::init();

        super::pic::Pic::init(
            0x20,
            0x21,
            0xa0,
            0xa1,
            0b00010001,
            0x02,
            0b0001_0101,
            0b0000_0001,
            0b0111_1111_1111_1010,
            [
                0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f, //
                0x70, 0x71, 0x72, 0x73, 0x74, 0x75, 0x76, 0x77,
            ],
        );

        super::pit::Pit::init(
            0x0040,
            0x0042,
            0x0043,
            11931, // 1.193_181_666MHz
            Irq(0),
            super::pit::Pit::advance_tick,
        );
        Hal::cpu().enable_interrupt();

        cga_text::CgaText::init_late();

        let _1mb = 0x0010_0000;
        let mut smap_supported = false;
        let buf = LoMemoryManager::alloc_page();
        let mut regs = Vm86Context::default();
        loop {
            regs.eax = 0xe820.into();
            regs.edx = 0x534d4150.into();
            regs.ecx = 24.into();
            regs.set_vmes(buf.sel());
            regs.edi.set_zero();
            bios::INT15.call(&mut regs);
            if regs.eflags().contains(Eflags::CF) || regs.eax.d() != 0x534d4150 {
                break;
            }
            smap_supported = true;

            let entry = &*(buf.as_slice().as_ptr() as *const SmapEntry);
            let range = entry.range();
            let mem_type = entry.mem_type();
            if range.start < _1mb && range.end <= _1mb {
                // low memory
                if mem_type != MemoryType::Available {
                    LoMemoryManager::reserve(range.start as usize..range.end as usize, mem_type)
                        .unwrap();
                }
            } else if range.start == _1mb {
                // already reported from SSBL
            } else {
                MemoryManager::register_memmap(range, mem_type).unwrap();
            }

            if regs.ebx.d() == 0 {
                break;
            }
        }

        if !smap_supported {
            // TODO: other way to detect memory size
        }

        if !USE_UART_STDIO {
            let _ = ps2::Ps2::init();
        }

        disk_bios::DiskBios::init();

        vesa_bios::VesaBios::init();
    }
}

pub(super) unsafe fn exit() {
    // to do nothing for now
}

pub(super) fn reset_system() -> ! {
    unsafe {
        // PCI reset
        IoPortWB(0x0CF9).write(0x06);

        // OADG reset
        LoIoPortWB::<0x92>::new().write(0x01);

        // PS/2 reset
        loop {
            let al = LoIoPortRB::<0x64>::new().read();
            if (al & 0x02) == 0 {
                break;
            }
        }
        LoIoPortWB::<0x64>::new().write(0xfe);

        Hal::cpu().halt();
    }
}

#[repr(C, packed)]
struct SmapEntry {
    base: u64,
    size: u64,
    attr: u32,
}

impl SmapEntry {
    pub fn range(&self) -> Range<u64> {
        self.base..(self.base + self.size)
    }

    pub fn mem_type(&self) -> MemoryType {
        match self.attr {
            1 => MemoryType::Available,
            3 => MemoryType::AcpiReclaim,
            4 => MemoryType::AcpiNvs,
            // 2 => MemoryType::Reserved,
            _ => MemoryType::Reserved,
        }
    }
}
