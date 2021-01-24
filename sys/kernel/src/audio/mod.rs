// Audio Manager

static mut BEEP_DRIVER: Option<&'static dyn BeepDriver> = None;

pub struct AudioManager {
    //
}

impl AudioManager {
    #[inline]
    pub(crate) unsafe fn set_beep(driver: &'static dyn BeepDriver) {
        BEEP_DRIVER = Some(driver);
    }

    pub fn make_beep(freq: usize) {
        unsafe {
            if let Some(driver) = BEEP_DRIVER {
                driver.make_beep(freq);
            }
        }
    }
}

pub trait BeepDriver {
    fn make_beep(&self, freq: usize);
}
