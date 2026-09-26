//! QEMU virt PLIC supervisor context for VirtIO MMIO interrupts.
use core::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

use crate::arch::csr::CSR;

static BASE: AtomicUsize = AtomicUsize::new(0);
static CONTEXT: AtomicUsize = AtomicUsize::new(0);
static MAX_SOURCE: AtomicU32 = AtomicU32::new(0);

fn read(base: usize, offset: usize) -> u32 {
    unsafe { ((base + offset) as *const u32).read_volatile() }
}

fn write(base: usize, offset: usize, value: u32) {
    unsafe { ((base + offset) as *mut u32).write_volatile(value) }
}

pub fn init(dt: &fdt::DeviceTree) -> bool {
    if !dt.root().is_compatible_with("riscv-virtio") {
        return false;
    }
    let found = fdt::bus::find_map(dt, |node, map| {
        if !node.is_compatible_with("sifive,plic-1.0.0") {
            return None;
        }
        let contexts = node.get_prop(fdt::PropName::INTERRUPTS_EXTENDED)?;
        let context = contexts
            .words()
            .chunks_exact(2)
            .position(|pair| pair[1].as_u32() == 9)?;
        Some((
            map.reg(node, 0),
            node.get_prop_u32(fdt::PropName("riscv,ndev")),
            context,
        ))
    });
    let Some((Some((base, size)), Some(max_source), context)) = found else {
        return false;
    };
    let Some(context_offset) = context
        .checked_mul(0x1000)
        .and_then(|v| 0x20_0000usize.checked_add(v))
    else {
        return false;
    };
    let Some(enable_end) = context
        .checked_mul(0x80)
        .and_then(|v| 0x2000usize.checked_add(v))
        .and_then(|v| v.checked_add((max_source as usize / 32) * 4 + 4))
    else {
        return false;
    };
    let Some(context_end) = context_offset.checked_add(8) else {
        return false;
    };
    if base > usize::MAX as u64
        || size > usize::MAX as u64
        || base
            .checked_add(size)
            .is_none_or(|end| end > usize::MAX as u64)
        || (base as usize) & 3 != 0
        || (size as usize) < context_end
        || (size as usize) < enable_end
        || max_source == 0
        || max_source > 1023
    {
        return false;
    }
    CONTEXT.store(context, Ordering::Relaxed);
    MAX_SOURCE.store(max_source, Ordering::Relaxed);
    BASE.store(base as usize, Ordering::Release);
    write(base as usize, context_offset, 0); // threshold
    true
}

pub fn register_source(source: u32) -> bool {
    let base = BASE.load(Ordering::Acquire);
    if base == 0 || source == 0 || source > MAX_SOURCE.load(Ordering::Relaxed) {
        return false;
    }
    let context = CONTEXT.load(Ordering::Relaxed);
    let enable = 0x2000 + context * 0x80 + (source as usize / 32) * 4;
    write(base, source as usize * 4, 1); // priority
    write(base, enable, read(base, enable) | (1 << (source % 32)));
    unsafe { CSR::SIE.set(1 << 9) }; // supervisor external interrupt
    true
}

pub fn handle_external() {
    let base = BASE.load(Ordering::Acquire);
    if base == 0 {
        return;
    }
    let claim = 0x20_0004 + CONTEXT.load(Ordering::Relaxed) * 0x1000;
    for _ in 0..32 {
        let source = read(base, claim);
        if source == 0 {
            break;
        }
        crate::io::virtio::irq::handle_source(source);
        write(base, claim, source);
    }
}
