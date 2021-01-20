// System

use crate::graphics::color::*;
use crate::graphics::coords::*;
use crate::graphics::emcon::*;
use crate::{fonts::*, graphics::bitmap::*};
use bootprot::*;
use core::fmt;
// use fmt::Write;

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version {
    versions: u32,
    rel: &'static str,
}

impl Version {
    const SYSTEM_NAME: &'static str = "Codename TOE";
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
    em_console: EmConsole,
}

#[used]
static mut SYSTEM: System = System::new();

impl System {
    const fn new() -> Self {
        Self {
            main_screen: None,
            em_console: EmConsole::new(),
        }
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

        bitmap.fill_rect(Rect::from(size), IndexedColor::WHITE);
        bitmap.fill_rect(Rect::new(0, 23, size.width(), 1), IndexedColor::BLACK);
        for y in 24..info.screen_height {
            for x in 0..info.screen_width {
                let point = Point::new(x as isize, y as isize);
                if ((x & y) & 3) == 3 {
                    bitmap.set_pixel(point, IndexedColor::BLACK);
                }
            }
        }

        let window_rect = Rect::new(10, 30, 200, 100);
        bitmap.fill_round_rect(window_rect, 8, IndexedColor::WHITE);
        bitmap.draw_round_rect(window_rect, 8, IndexedColor::BLACK);
        bitmap.draw_circle(Point::new(100, 150), 48, IndexedColor::BLUE);
        bitmap.draw_circle(Point::new(150, 200), 49, IndexedColor::RED);
        bitmap.draw_circle(Point::new(200, 150), 50, IndexedColor::GREEN);

        bitmap.view(window_rect, |bitmap| {
            bitmap.draw_round_rect(Rect::new(50, 50, 200, 200), 8, IndexedColor::BLUE);
        });

        let font = FontManager::fixed_system_font();
        font.write_str("Hello", bitmap, Point::new(10, 4), IndexedColor::RED);

        font.write_str("Welcome!", bitmap, Point::new(20, 40), IndexedColor::BLUE);

        unimplemented!();
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

    /// Get main screen
    pub fn main_screen() -> &'static mut OsMutBitmap8<'static> {
        let shared = Self::shared();
        shared.main_screen.as_mut().unwrap()
    }

    /// Get emergency console
    pub fn em_console<'a>() -> &'a mut EmConsole {
        let shared = Self::shared();
        &mut shared.em_console
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
