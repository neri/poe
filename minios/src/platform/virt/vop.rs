//! Rockchip VOP (Video Output Processor)
//!
//! depthcharge puts the VOP used for the firmware screen into standby when it hands off
//! to the OS, so the framebuffer is not displayed until the VOP is woken up.

use super::find_reg;

const REG_CFG_DONE: usize = 0x0000;
const SYS_CTRL: usize = 0x0008;
const WIN0_YRGB_MST: usize = 0x0040;

const SYS_CTRL_STANDBY: u32 = 1 << 22;

/// State of the VOP before waking it up
pub struct VopState {
    pub base: usize,
    pub was_standby: bool,
    /// Address of the framebuffer being scanned out
    pub scanout: u32,
}

/// Returns the address of the framebuffer scanned out by VOP big.
///
/// depthcharge programs it with the framebuffer allocated by libpayload.
pub unsafe fn scanout_address(dt: &fdt::DeviceTree) -> Option<usize> {
    let (base, _size) = find_reg(dt, &["rockchip,rk3399-vop-big"], 0)?;
    let scanout = unsafe { ((base + WIN0_YRGB_MST) as *const u32).read_volatile() };
    (scanout != 0).then_some(scanout as usize)
}

/// Wakes up VOP big from standby, the same way as depthcharge does.
///
/// VOP big is used by coreboot for the firmware screen.
/// VOP lit is not touched, since it may be powered off and accessing it may hang.
pub unsafe fn wake_up(dt: &fdt::DeviceTree) -> Option<VopState> {
    let (base, _size) = find_reg(dt, &["rockchip,rk3399-vop-big"], 0)?;
    let reg = |offset: usize| (base + offset) as *mut u32;
    unsafe {
        let sys_ctrl = reg(SYS_CTRL).read_volatile();
        let state = VopState {
            base,
            was_standby: (sys_ctrl & SYS_CTRL_STANDBY) != 0,
            scanout: reg(WIN0_YRGB_MST).read_volatile(),
        };
        reg(SYS_CTRL).write_volatile(sys_ctrl & !SYS_CTRL_STANDBY);
        reg(REG_CFG_DONE).write_volatile(0xffff);
        Some(state)
    }
}
