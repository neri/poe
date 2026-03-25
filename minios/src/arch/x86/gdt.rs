//! Global Descriptor Table

use crate::arch::bits::BitArray;
use crate::arch::cpu::SetDescriptorError;
use core::arch::asm;
use core::cell::UnsafeCell;
use core::mem::offset_of;
use core::ptr;
use core::sync::atomic::{Ordering, compiler_fence};
use x86::gpr::Gpr32;
use x86::prot::*;
use x86::real::Offset16;

pub const SYSTEM_TSS: Selector = Selector::new(1, RPL0);
pub const KERNEL_CSEL: Selector = Selector::new(2, RPL0);
pub const KERNEL_DSEL: Selector = Selector::new(3, RPL0);
pub const USER_CSEL: Selector = Selector::new(4, RPL3);
pub const USER_DSEL: Selector = Selector::new(5, RPL3);

static mut GDT: UnsafeCell<Gdt> = UnsafeCell::new(Gdt::new());

/// Global Descriptor Table
#[repr(C, align(16))]
pub struct Gdt {
    table: [DescriptorEntry; Self::NUM_ITEMS],
    tss: TaskStateSegment32,
    iopb: BitArray<{ 65536 / 32 }>,
}

impl Gdt {
    pub const NUM_ITEMS: usize = 16;

    #[inline]
    const fn new() -> Self {
        let mut gdt = Self {
            table: [DescriptorEntry::NULL; Self::NUM_ITEMS],
            tss: TaskStateSegment32::empty(),
            iopb: BitArray::new(),
        };

        unsafe {
            gdt._set_item(KERNEL_CSEL, SegmentDescriptor::flat_code32(DPL0));
            gdt._set_item(KERNEL_DSEL, SegmentDescriptor::flat_data(DPL0));

            gdt._set_item(USER_CSEL, SegmentDescriptor::flat_code32(DPL3));
            gdt._set_item(USER_DSEL, SegmentDescriptor::flat_data(DPL3));
        }

        gdt
    }

    #[inline]
    pub unsafe fn shared<'a>() -> &'a mut Self {
        unsafe { (&mut *(&raw mut GDT)).get_mut() }
    }

    pub(super) unsafe fn init() {
        unsafe {
            let gdt = Self::shared();

            gdt.tss.ss0 = KERNEL_DSEL.into();
            let iopb_base = (offset_of!(Self, iopb) - offset_of!(Self, tss)) as u16;
            gdt.tss.iopb_base = Offset16::new(iopb_base);
            let tss_base = Linear32::new(&gdt.tss as *const _ as u32);
            let tss_limit = Limit16::new(iopb_base + 8191);
            gdt._set_item(SYSTEM_TSS, SegmentDescriptor::tss32(tss_base, tss_limit));

            gdt.reload();

            // SSBL starts with a temporary GDT, so reload the selector based on our new GDT here
            asm!(
                "mov ss, {new_ss:e}",
                "push {new_cs:e}",
                // trampoline code to set new cs register
                //      call _retf
                //      jmp _next
                // _retf:
                //      retf
                // _next:
                ".byte 0xe8, 2, 0, 0, 0, 0xeb, 0x01, 0xcb",

                "mov ds, {new_ss:e}",
                "mov es, {new_ss:e}",
                "mov fs, {new_ss:e}",
                "mov gs, {new_ss:e}",
                new_ss = in(reg) KERNEL_DSEL.as_usize(),
                new_cs = in(reg) KERNEL_CSEL.as_usize(),
            );

            asm!("ltr {0:x}", in(reg) SYSTEM_TSS.0,);
        }
    }

    #[inline]
    pub const unsafe fn set_item(
        &mut self,
        selector: Selector,
        desc: DescriptorEntry,
    ) -> Result<(), SetDescriptorError> {
        if selector.rpl().ne(&desc.dpl().as_rpl()) {
            return Err(SetDescriptorError::PriviledgeMismatch);
        }
        let index = selector.index();
        if index < self.table.len() {
            self.table[index] = desc;
            Ok(())
        } else {
            Err(SetDescriptorError::OutOfIndex)
        }
    }

    #[inline]
    #[track_caller]
    const unsafe fn _set_item(&mut self, selector: Selector, desc: DescriptorEntry) {
        unsafe {
            match self.set_item(selector, desc) {
                Ok(()) => (),
                Err(_) => panic!("Failed to set GDT selector"),
            }
        }
    }

    /// Reload GDT
    unsafe fn reload(&self) {
        compiler_fence(Ordering::SeqCst);
        unsafe {
            asm!(
                "push {0}",
                "push {1}",
                "lgdt [esp + 2]",
                "add esp, 8",
                in(reg) &self.table,
                in(reg) ((self.table.len() * 8 - 1) << 16),
            );
        }
        compiler_fence(Ordering::SeqCst);
    }

    #[inline]
    pub fn tss_mut(&mut self) -> &mut TaskStateSegment32 {
        &mut self.tss
    }

    #[inline]
    pub unsafe fn set_tss_esp0(esp: Gpr32) {
        compiler_fence(Ordering::SeqCst);
        unsafe {
            let tss = Self::shared().tss_mut();
            ptr::addr_of_mut!(tss.esp0).write_volatile(esp);
        }
        compiler_fence(Ordering::SeqCst);
    }

    #[inline]
    pub fn get_tss_esp0() -> Gpr32 {
        unsafe {
            let tss = Self::shared().tss_mut();
            ptr::addr_of!(tss.esp0).read_volatile()
        }
    }
}
