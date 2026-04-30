//! **IA32_EFER**: Extended Feature Enables Register *(MSR C000_0080)*

use crate::msr::MSR;
use crate::view::{ControlRegisterReadonlyView, ControlRegisterView};

/// **IA32_EFER**: Extended Feature Enables Register *(MSR C000_0080)*
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EFER(u64);

#[allow(non_snake_case)]
impl EFER {
    /// Enables the `syscall` and `sysret` instructions
    pub fn SYSCALL<'a>(&'a mut self) -> ControlRegisterView<'a, u64> {
        ControlRegisterView::new(&mut self.0, 1 << 0)
    }

    /// Activates long mode
    pub fn LME<'a>(&'a mut self) -> ControlRegisterView<'a, u64> {
        ControlRegisterView::new(&mut self.0, 1 << 8)
    }

    /// Indicates that long mode is active. THIS BIT CANNOT BE CHANGED MANUALLY.
    pub fn LMA<'a>(&'a self) -> ControlRegisterReadonlyView<'a, u64> {
        ControlRegisterReadonlyView::new(&self.0, 1 << 10)
    }

    /// Enables the no-execute page-protection feature
    pub fn NXE<'a>(&'a mut self) -> ControlRegisterView<'a, u64> {
        ControlRegisterView::new(&mut self.0, 1 << 11)
    }

    #[inline]
    pub fn fetch() -> Self {
        unsafe { Self(MSR::IA32_EFER.read()) }
    }

    #[inline]
    pub unsafe fn update(&self) {
        unsafe {
            MSR::IA32_EFER.write(self.0);
        }
    }

    #[inline]
    pub fn fetch_update<F>(f: F)
    where
        F: FnOnce(&mut Self),
    {
        let mut efer = Self::fetch();
        f(&mut efer);
        unsafe { efer.update() };
    }
}
