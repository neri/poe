// System

use crate::graphics::bitmap::*;
use crate::graphics::color::*;
use crate::graphics::coords::*;
use bootprot::*;
use core::fmt;

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version {
    versions: u32,
    rel: &'static str,
}

impl Version {
    const SYSTEM_NAME: &'static str = "MEG-OS";
    const RELEASE: &'static str = "";
    const VERSION: Version = Version::new(0, 0, 1, Self::RELEASE);

    const fn new(maj: u8, min: u8, patch: u16, rel: &'static str) -> Self {
        let versions = ((maj as u32) << 24) | ((min as u32) << 16) | (patch as u32);
        Version { versions, rel }
    }

    pub const fn as_u32(&self) -> u32 {
        self.versions
    }

    pub const fn maj(&self) -> usize {
        ((self.versions >> 24) & 0xFF) as usize
    }

    pub const fn min(&self) -> usize {
        ((self.versions >> 16) & 0xFF) as usize
    }

    pub const fn patch(&self) -> usize {
        (self.versions & 0xFFFF) as usize
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.rel.len() > 0 {
            write!(
                f,
                "{}.{}.{}-{}",
                self.maj(),
                self.min(),
                self.patch(),
                self.rel
            )
        } else {
            write!(f, "{}.{}.{}", self.maj(), self.min(), self.patch())
        }
    }
}

pub struct System {
    main_screen: Option<OsMutBitmap8<'static>>,
}

#[used]
static mut SYSTEM: System = System::new();

impl System {
    const fn new() -> Self {
        Self { main_screen: None }
    }

    #[inline]
    pub unsafe fn init(info: &BootInfo, _f: fn() -> ()) -> ! {
        let shared = Self::shared();

        let size = Size::new(info.screen_width as isize, info.screen_height as isize);
        let stride = info.screen_stride as usize;
        shared.main_screen = Some(OsMutBitmap8::from_static(
            info.vram_base as usize as *mut IndexedColor,
            size,
            stride,
        ));

        let bitmap = shared.main_screen.as_mut().unwrap();

        // bitmap.fill_rect(Rect::from(size), IndexedColor::WHITE);
        for y in 0..info.screen_height {
            for x in 0..info.screen_width {
                let point = Point::new(x as isize, y as isize);
                let color = if ((x ^ y) & 1) == 0 {
                    IndexedColor::BLACK
                } else {
                    IndexedColor::WHITE
                };
                bitmap.set_pixel(point, color);
            }
        }

        bitmap.fill_round_rect(Rect::new(50, 50, 200, 200), 8, IndexedColor::WHITE);
        bitmap.draw_round_rect(Rect::new(50, 50, 200, 200), 8, IndexedColor::BLACK);
        bitmap.draw_circle(Point::new(100, 100), 48, IndexedColor::BLUE);
        bitmap.draw_circle(Point::new(150, 150), 49, IndexedColor::RED);
        bitmap.draw_circle(Point::new(200, 100), 50, IndexedColor::GREEN);

        loop {
            asm!("hlt");
        }
    }

    /// Returns an internal shared instance
    #[inline]
    fn shared() -> &'static mut System {
        unsafe { &mut SYSTEM }
    }

    /// Returns the name of current system.
    #[inline]
    pub const fn name() -> &'static str {
        &Version::SYSTEM_NAME
    }

    /// Returns the version of current system.
    #[inline]
    pub const fn version() -> &'static Version {
        &Version::VERSION
    }

    /// SAFETY: IT DESTROYS EVERYTHING.
    pub unsafe fn reset() -> ! {
        todo!();
    }

    /// SAFETY: IT DESTROYS EVERYTHING.
    pub unsafe fn shutdown() -> ! {
        todo!();
    }

    // #[inline]
    // pub fn uarts<'a>() -> &'a [Box<dyn Uart>] {
    //     arch::Arch::uarts()
    // }
}

#[derive(Debug, Copy, Clone)]
pub struct SystemTime {
    pub secs: u64,
    pub nanos: u32,
}
