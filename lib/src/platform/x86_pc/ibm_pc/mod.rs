//! IBM PC architecture specific code

pub mod bios;
mod cga_text;
mod disk_bios;
// mod ps2;

use crate::{
    arch::{cpu::X86StackContext, lomem::LoMemoryManager, vm86::VM86},
    mem::{MemoryManager, MemoryType},
    *,
};
use acpi::{ACPI_10_TABLE_GUID, ACPI_20_TABLE_GUID, RsdPtr, RsdPtrV1};
use core::{ffi::c_void, iter::Iterator, ops::Range};
use smbios::{SMBIOS_GUID, SmBios};
use x86::gpr::Eflags;

pub(super) unsafe fn init_early() {
    unsafe {
        cga_text::CgaText::init();

        let ebda = ((0x40e as *const u16).read_volatile() as u32) << 4;

        // find ACPI RSD Ptr tables
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
            System::add_config_table_entry(ACPI_10_TABLE_GUID, acpi1);
        }
        if let Some(acpi2) = acpi2 {
            System::add_config_table_entry(ACPI_20_TABLE_GUID, acpi2);
        }

        // find SMBIOS entry
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
            System::add_config_table_entry(SMBIOS_GUID, smbios);
        }
    }
}

pub(super) unsafe fn init_late() {
    // let info = Environment::boot_info();
    unsafe {
        let mut smap_supported = false;
        let buf = LoMemoryManager::alloc_page();
        let mut regs = X86StackContext::default();
        loop {
            regs.eax = 0xe820;
            regs.edx = 0x534d4150;
            regs.ecx = 24;
            regs.set_vmes(buf.sel());
            regs.edi = 0;
            VM86::call_bios(bios::INT15, &mut regs);
            if regs.eflags.contains(Eflags::CF) || regs.eax != 0x534d4150 {
                break;
            }
            smap_supported = true;

            let entry = &*(buf.as_slice().as_ptr() as *const SmapEntry);
            let range = entry.range();
            if let Some(mem_type) = entry.mem_type() {
                if range.start < 0x10_0000 && range.end <= 0x10_0000 {
                    if mem_type != MemoryType::Available {
                        LoMemoryManager::reserve(
                            range.start as usize..range.end as usize,
                            mem_type,
                        )
                        .unwrap();
                    }
                } else if range.start == 0x10_0000 {
                    // reported from SSBL
                } else {
                    MemoryManager::register_memmap(range, mem_type).unwrap();
                }
            }

            if regs.ebx == 0 {
                break;
            }
        }

        if !smap_supported {
            // TODO:
        }

        let kbd = &mut *(&raw mut STDIN);
        kbd.reset();
        System::set_stdin(kbd);

        disk_bios::DiskBios::init();
    }
}

pub(super) unsafe fn exit() {
    // TODO:
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

    pub fn mem_type(&self) -> Option<MemoryType> {
        match self.attr {
            1 => Some(MemoryType::Available),
            2 => Some(MemoryType::Reserved),
            3 => Some(MemoryType::AcpiReclaim),
            4 => Some(MemoryType::AcpiNvs),
            _ => None,
        }
    }
}

static mut STDIN: BiosTextInput = BiosTextInput {};

struct BiosTextInput;

impl SimpleTextInput for BiosTextInput {
    fn reset(&mut self) {
        while self.read_key_stroke().is_some() {}
    }

    fn read_key_stroke(&mut self) -> Option<NonZeroInputKey> {
        unsafe {
            let head = 0x41a as *const u16;
            let tail = 0x41c as *const u16;
            if head.read_volatile() == tail.read_volatile() {
                return None;
            }
            let mut regs = X86StackContext::default();
            regs.eax = 0;
            VM86::call_bios(bios::INT16, &mut regs);
            InputKey {
                usage: (regs.eax >> 8) as u16,
                unicode_char: (regs.eax & 0xFF) as u16,
            }
            .into()
        }
    }
}
