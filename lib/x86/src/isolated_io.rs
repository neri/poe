//! Isolated I/O operations.

use core::arch::asm;
use paste::paste;

macro_rules! lo_ioport {
    ($suffix:ident, $val_type:ident, $reg_acc:literal, $in_op:literal, $out_op:literal) => {
        paste! {
            #[derive(Debug, Clone, Copy, PartialEq, Eq)]
            pub struct [<LoIoPortRW $suffix>]<const PORT: u16>;

            impl<const PORT: u16> [<LoIoPortRW $suffix>]<PORT> {
                #[inline(always)]
                pub const fn new() -> Self {
                    Self
                }

                #[inline(always)]
                pub unsafe fn read(&self) -> $val_type {
                    let al;
                    unsafe {
                        asm!(
                            $in_op,
                            port = const PORT,
                            out($reg_acc) al,
                        );
                    }
                    al
                }

                #[inline(always)]
                pub unsafe fn write(&self, value: $val_type) {
                    unsafe {
                        asm!(
                            $out_op,
                            port = const PORT,
                            in($reg_acc) value,
                        );
                    }
                }
            }

            #[derive(Debug, Clone, Copy, PartialEq, Eq)]
            pub struct [<LoIoPortR $suffix>]<const PORT: u16>;

            impl<const PORT: u16> [<LoIoPortR $suffix>]<PORT> {
                #[inline(always)]
                pub const fn new() -> Self {
                    Self
                }

                #[inline(always)]
                pub unsafe fn read(&self) -> $val_type {
                    unsafe {
                        [<LoIoPortRW $suffix>]::<PORT>::new().read()
                    }
                }
            }

            #[derive(Debug, Clone, Copy, PartialEq, Eq)]
            pub struct [<LoIoPortW $suffix>]<const PORT: u16>;

            impl<const PORT: u16> [<LoIoPortW $suffix>]<PORT> {
                #[inline(always)]
                pub const fn new() -> Self {
                    Self
                }

                #[inline(always)]
                pub unsafe fn write(&self, value: $val_type) {
                    unsafe {
                        [<LoIoPortRW $suffix>]::<PORT>::new().write(value);
                    }
                }
            }

            #[derive(Debug, Clone, Copy, PartialEq, Eq)]
            pub struct [<LoIoPortDummy $suffix>]<const PORT: u16>;

            impl<const PORT: u16> [<LoIoPortDummy $suffix>]<PORT> {
                #[inline(always)]
                pub const fn new() -> Self {
                    Self
                }

                #[inline(always)]
                pub unsafe fn write_dummy(&self) {
                    unsafe {
                        asm!(
                            $out_op,
                            port = const PORT,
                        );
                    }
                }
            }

        }
    };
}

macro_rules! ioport {
    ($suffix:ident, $val_type:ident, $reg_acc:literal, $in_op:literal, $out_op:literal) => {
        paste! {
            #[derive(Debug, Clone, Copy, PartialEq, Eq)]
            pub struct [<IoPortRW $suffix>](pub u16);

            impl [<IoPortRW $suffix>] {
                #[inline(always)]
                pub unsafe fn read(&self) -> $val_type {
                    unsafe {
                        [<IoPortR $suffix>]::from(*self).read()
                    }
                }

                #[inline(always)]
                pub unsafe fn write(&self, value: $val_type) {
                    unsafe {
                        [<IoPortW $suffix>]::from(*self).write(value);
                    }
                }
            }

            #[derive(Debug, Clone, Copy, PartialEq, Eq)]
            pub struct [<IoPortR $suffix>](pub u16);

            impl [<IoPortR $suffix>] {
                #[inline(always)]
                pub unsafe fn read(&self) -> $val_type {
                    let al;
                    unsafe {
                        asm!(
                            $in_op,
                            in("dx") self.0,
                            out($reg_acc) al,
                        );
                    }
                    al
                }
            }

            impl From<[<IoPortRW $suffix>]> for [<IoPortR $suffix>] {
                #[inline(always)]
                fn from(port: [<IoPortRW $suffix>]) -> Self {
                    Self(port.0)
                }
            }

            #[derive(Debug, Clone, Copy, PartialEq, Eq)]
            pub struct [<IoPortW $suffix>](pub u16);

            impl [<IoPortW $suffix>] {
                #[inline(always)]
                pub unsafe fn write(&self, value: $val_type) {
                    unsafe {
                        asm!(
                            $out_op,
                            in("dx") self.0,
                            in($reg_acc) value,
                        );
                    }
                }
            }

            impl From<[<IoPortRW $suffix>]> for [<IoPortW $suffix>] {
                #[inline(always)]
                fn from(port: [<IoPortRW $suffix>]) -> Self {
                    Self(port.0)
                }
            }
        }
    };
}

lo_ioport!(B, u8, "al", "in al, {port}", "out {port}, al");

ioport!(B, u8, "al", "in al, dx", "out dx, al");
ioport!(W, u16, "ax", "in ax, dx", "out dx, ax");
ioport!(D, u32, "eax", "in eax, dx", "out dx, eax");
