// System

use crate::graphics::color::*;
use crate::graphics::coords::*;
use crate::graphics::emcon::*;
use crate::*;
use crate::{fonts::*, graphics::bitmap::*};
use bootprot::*;
use core::fmt;
use fmt::Write;

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version {
    versions: u32,
    rel: &'static str,
}

impl Version {
    const SYSTEM_NAME: &'static str = "codename TOE";
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

        arch::Arch::init();

        let bitmap = shared.main_screen.as_mut().unwrap();

        bitmap.fill_rect(Rect::from(size), IndexedColor::LIGHT_CYAN);
        bitmap.fill_rect(Rect::new(0, 0, size.width(), 24), IndexedColor::LIGHT_GRAY);
        bitmap.draw_hline(Point::new(0, 22), size.width(), IndexedColor::DARK_GRAY);
        bitmap.draw_hline(Point::new(0, 23), size.width(), IndexedColor::BLACK);

        let font = FontManager::fixed_system_font();

        {
            let window_rect = Rect::new(20, 40, 200, 100);
            bitmap.fill_round_rect(window_rect, 8, IndexedColor::LIGHT_GRAY);
            bitmap.view(window_rect, |bitmap| {
                let title_rect = Rect::new(0, 0, 200, 24);
                bitmap.view(title_rect, |bitmap| {
                    bitmap.fill_round_rect(Rect::new(0, 0, 200, 40), 8, IndexedColor::BLUE);
                    font.write_str("Hello", bitmap, Point::new(8, 4), IndexedColor::WHITE);
                });
                font.write_str("It works!", bitmap, Point::new(10, 40), IndexedColor::BLACK);
            });
            bitmap.draw_round_rect(window_rect, 8, IndexedColor::BLACK);
        }

        {
            let window_rect = Rect::new(240, 200, 160, 100);
            let coords = Coordinates::from_rect_unchecked(window_rect);
            bitmap.fill_rect(window_rect, IndexedColor::LIGHT_GRAY);

            bitmap.draw_hline(
                coords.left_top() + Point::new(2, 2),
                window_rect.width() - 4,
                IndexedColor::WHITE,
            );
            bitmap.draw_vline(
                coords.left_top() + Point::new(2, 2),
                window_rect.height() - 4,
                IndexedColor::WHITE,
            );
            bitmap.draw_vline(
                coords.right_top() + Point::new(-2, 2),
                window_rect.height() - 4,
                IndexedColor::DARK_GRAY,
            );
            bitmap.draw_hline(
                coords.left_bottom() + Point::new(2, -2),
                window_rect.width() - 4,
                IndexedColor::DARK_GRAY,
            );
            bitmap.draw_rect(window_rect, IndexedColor::BLACK);
        }

        println!(
            "{} v{} Memory {} KB",
            System::name(),
            System::version(),
            info.memsz_mi
        );

        // unimplemented!();
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
