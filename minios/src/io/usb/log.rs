//! Where USB diagnostics go.
//!
//! Not to the active console. POE switches that to the framebuffer once it
//! starts, and the USB stack keeps working in the background, so a line
//! printed from a hotplug or a retry would land in the middle of whatever the
//! application is drawing — and would never reach the serial log, which is
//! where this kind of problem is actually read. On a board with a PL011 from
//! the device tree the output goes straight to it instead.

#[allow(unused_variables)]
pub fn write(args: core::fmt::Arguments) {
    #[cfg(feature = "arm64dt")]
    {
        use core::fmt::Write;

        use crate::platform::arm64dt::pl011::Pl011;
        if Pl011::is_initialized() {
            let _ = writeln!(Pl011::shared(), "{args}");
            return;
        }
    }
    let _ = writeln!(crate::System::stdout(), "{args}");
}
