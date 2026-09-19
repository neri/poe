//! Boot progress markers for RK3399 Chromebooks (diagnostic build only)
//!
//! Draws a white band per boot stage on the screen scanned out by VOP big,
//! so that the stage where the boot stops can be seen without UART.
//! White (all ones) is visible in any pixel format.
//!
//! The VOP address is hardcoded, since the device tree may not be usable yet.
//!
//! | band (from the top) | meaning                                  |
//! |---------------------|------------------------------------------|
//! | 0 - 6               | boot stages passed                       |
//! | 7                   | framebuffer found in the coreboot table  |
//! | 8                   | DTB: x0 is 0 or not 4-byte aligned       |
//! | 9                   | DTB: bad magic                           |
//! | 10                  | DTB: version is not 17 / last_comp 16    |
//! | 11                  | DTB: the first token is not the root     |
//! | 12                  | failed to turn on the backlight (EC)     |
//! | 13                  | panic                                    |
//! | 14                  | CPU exception                            |
//! | 15                  | framebuffer not found                    |

/// VOP big of RK3399
const VOP_BASE: usize = 0xff90_0000;
const REG_CFG_DONE: usize = 0x0000;
const SYS_CTRL: usize = 0x0008;
const WIN0_VIR: usize = 0x003c;
const WIN0_YRGB_MST: usize = 0x0040;
const WIN0_ACT_INFO: usize = 0x0048;

const SYS_CTRL_STANDBY: u32 = 1 << 22;

const BANDS: usize = 16;

pub const STAGE_FB_FOUND: usize = 7;
pub const STAGE_DTB_POINTER: usize = 8;
pub const STAGE_DTB_MAGIC: usize = 9;
pub const STAGE_DTB_VERSION: usize = 10;
pub const STAGE_DTB_ROOT: usize = 11;
pub const STAGE_BACKLIGHT_FAILED: usize = 12;
pub const STAGE_PANIC: usize = 13;
pub const STAGE_EXCEPTION: usize = 14;
pub const STAGE_FB_NOT_FOUND: usize = 15;

/// Checks the device tree blob with the same conditions as the fdt library,
/// and draws the band of the first problem found.
pub unsafe fn check_dtb(dtb: usize) {
    if dtb == 0 || (dtb & 3) != 0 {
        unsafe { mark(STAGE_DTB_POINTER) };
        return;
    }
    let read =
        |offset: usize| u32::from_be(unsafe { ((dtb + offset) as *const u32).read_volatile() });
    if read(0x00) != 0xd00d_feed {
        unsafe { mark(STAGE_DTB_MAGIC) };
        return;
    }
    if read(0x14) != 17 || read(0x18) != 16 {
        unsafe { mark(STAGE_DTB_VERSION) };
        return;
    }
    // FDT_NOP tokens are skipped, then FDT_BEGIN_NODE with an empty name
    let mut offset = read(0x08) as usize;
    for _ in 0..16 {
        if read(offset) != 4 {
            break;
        }
        offset += 4;
    }
    let name = unsafe { ((dtb + offset + 4) as *const u8).read_volatile() };
    if read(offset) != 1 || name != 0 {
        unsafe { mark(STAGE_DTB_ROOT) };
    }
}

/// Draws the band of `stage`. Uses no static variables and no heap.
pub unsafe fn mark(stage: usize) {
    let reg = |offset: usize| (VOP_BASE + offset) as *mut u32;
    unsafe {
        reg(SYS_CTRL).write_volatile(reg(SYS_CTRL).read_volatile() & !SYS_CTRL_STANDBY);
        reg(REG_CFG_DONE).write_volatile(0xffff);

        let fb = reg(WIN0_YRGB_MST).read_volatile() as usize;
        let stride = (reg(WIN0_VIR).read_volatile() & 0x3fff) as usize * 4;
        let height = ((reg(WIN0_ACT_INFO).read_volatile() >> 16) & 0x1fff) as usize + 1;
        if fb == 0 || (fb & 3) != 0 || stride == 0 {
            return;
        }

        let band = height / BANDS;
        let top = stage.min(BANDS - 1) * band;
        for y in top..top + band / 2 {
            let line = fb + y * stride;
            for x in (0..stride).step_by(4) {
                ((line + x) as *mut u32).write_volatile(!0);
            }
        }
    }
}
