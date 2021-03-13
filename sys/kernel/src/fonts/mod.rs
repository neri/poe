// Fonts

use super::graphics::bitmap::*;
use crate::graphics::color::*;
use crate::graphics::coords::*;
use alloc::boxed::Box;
use alloc::collections::BTreeMap;

include!("megh0816.rs");
const SYSTEM_FONT: FixedFontDriver = FixedFontDriver::new(8, 16, &FONT_MEGH0816_DATA, None);

include!("megh0608.rs");
const SMALL_FONT: FixedFontDriver = FixedFontDriver::new(6, 8, &FONT_MEGH0608_DATA, None);

include!("megmsgr2.rs");
const SYSTEM_UI_FONT: FixedFontDriver =
    FixedFontDriver::new(8, 12, &FONT_MEGMSGR2_DATA, Some(SYSTEM_UI_WIDTH_TABLE));

const SYSTEM_UI_WIDTH_TABLE: [u8; 96] = [
    6, 4, 6, 7, 6, 7, 7, 3, 4, 4, 6, 6, 3, 6, 3, 5, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 5, 5, 5, 6, 5, 6,
    7, 7, 7, 7, 7, 6, 6, 7, 7, 4, 6, 7, 6, 8, 7, 7, 7, 7, 7, 7, 6, 7, 6, 8, 7, 6, 7, 4, 5, 4, 7, 6,
    3, 6, 6, 6, 6, 6, 5, 6, 6, 5, 5, 6, 5, 8, 6, 6, 6, 6, 5, 6, 5, 6, 6, 8, 6, 6, 6, 4, 2, 4, 6, 6,
];

static mut FONT_MANAGER: FontManager = FontManager::new();

pub struct FontManager {
    fonts: Option<BTreeMap<FontFamily, Box<dyn FontDriver>>>,
}

impl FontManager {
    const fn new() -> Self {
        Self { fonts: None }
    }

    #[inline]
    fn shared<'a>() -> &'a mut Self {
        unsafe { &mut FONT_MANAGER }
    }

    pub(crate) fn init() {
        let shared = Self::shared();

        let mut fonts: BTreeMap<FontFamily, Box<dyn FontDriver>> = BTreeMap::new();

        fonts.insert(FontFamily::FixedSystem, Box::new(SYSTEM_FONT));
        fonts.insert(FontFamily::SmallFixed, Box::new(SMALL_FONT));
        fonts.insert(FontFamily::SystemUI, Box::new(SYSTEM_UI_FONT));

        shared.fonts = Some(fonts);
    }

    fn driver_for(family: FontFamily) -> Option<&'static dyn FontDriver> {
        let shared = Self::shared();
        shared
            .fonts
            .as_ref()
            .and_then(|v| v.get(&family))
            .map(|v| v.as_ref())
    }

    #[inline]
    pub const fn fixed_system_font() -> &'static FixedFontDriver<'static> {
        &SYSTEM_FONT
    }

    #[inline]
    pub const fn fixed_ui_font() -> &'static FixedFontDriver<'static> {
        &SYSTEM_UI_FONT
    }

    #[inline]
    pub const fn fixed_small_font() -> &'static FixedFontDriver<'static> {
        &SMALL_FONT
    }

    #[inline]
    pub fn system_font() -> FontDescriptor {
        FontDescriptor::new(FontFamily::FixedSystem, 0).unwrap()
    }

    #[inline]
    pub fn title_font() -> FontDescriptor {
        FontDescriptor::new(FontFamily::SystemUI, 0).unwrap_or(Self::system_font())
    }

    #[inline]
    pub fn ui_font() -> FontDescriptor {
        FontDescriptor::new(FontFamily::SystemUI, 0).unwrap_or(Self::system_font())
    }
}

#[non_exhaustive]
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum FontFamily {
    SystemUI,
    FixedSystem,
    SmallFixed,
}

#[derive(Copy, Clone)]
pub struct FontDescriptor {
    driver: &'static dyn FontDriver,
    point: i32,
    line_height: i32,
}

impl FontDescriptor {
    pub fn new(family: FontFamily, point: isize) -> Option<Self> {
        FontManager::driver_for(family).map(|driver| {
            if driver.is_scalable() {
                Self {
                    driver,
                    point: point as i32,
                    line_height: (driver.preferred_line_height() * point / driver.base_height())
                        as i32,
                }
            } else {
                Self {
                    driver,
                    point: driver.base_height() as i32,
                    line_height: driver.preferred_line_height() as i32,
                }
            }
        })
    }

    #[inline]
    pub const fn point(&self) -> isize {
        self.point as isize
    }

    #[inline]
    pub const fn line_height(&self) -> isize {
        self.line_height as isize
    }

    #[inline]
    pub fn width_of(&self, character: char) -> isize {
        if self.point() == self.driver.base_height() {
            self.driver.width_of(character)
        } else {
            self.driver.width_of(character) * self.point() / self.driver.base_height()
        }
    }

    #[inline]
    pub fn is_scalable(&self) -> bool {
        self.driver.is_scalable()
    }

    #[inline]
    pub fn draw_char(
        &self,
        character: char,
        bitmap: &mut Bitmap,
        origin: Point,
        color: AmbiguousColor,
    ) {
        self.driver
            .draw_char(character, bitmap, origin, self.point(), color)
    }
}

pub trait FontDriver {
    fn is_scalable(&self) -> bool;

    fn base_height(&self) -> isize;

    fn preferred_line_height(&self) -> isize;

    fn width_of(&self, character: char) -> isize;

    fn height_of(&self, character: char) -> isize;

    fn draw_char(
        &self,
        character: char,
        bitmap: &mut Bitmap,
        origin: Point,
        height: isize,
        color: AmbiguousColor,
    );
}

pub struct FixedFontDriver<'a> {
    size: Size,
    data: &'a [u8],
    leading: isize,
    line_height: isize,
    stride: usize,
    width_table: Option<[u8; 96]>,
}

impl FixedFontDriver<'_> {
    pub const fn new(
        width: usize,
        height: usize,
        data: &'static [u8],
        width_table: Option<[u8; 96]>,
    ) -> FixedFontDriver<'static> {
        let width = width as isize;
        let height = height as isize;
        let line_height = height * 5 / 4;
        let leading = (line_height - height) / 2;
        let stride = ((width as usize + 7) >> 3) * height as usize;
        FixedFontDriver {
            size: Size::new(width, height),
            line_height,
            leading,
            stride,
            data,
            width_table,
        }
    }

    #[inline]
    pub const fn width(&self) -> isize {
        self.size.width
    }

    #[inline]
    pub const fn line_height(&self) -> isize {
        self.line_height
    }

    #[inline]
    pub fn height_for(&self, character: char) -> isize {
        let _ = character;
        self.size.height
    }

    /// Glyph Data for Rasterized Font
    pub fn glyph_for(&self, character: char) -> Option<&[u8]> {
        let c = character as usize;
        if c > 0x20 && c < 0x80 {
            let base = self.stride * (c - 0x20);
            Some(&self.data[base..base + self.stride])
        } else {
            None
        }
    }
}

impl FontDriver for FixedFontDriver<'_> {
    #[inline]
    fn is_scalable(&self) -> bool {
        false
    }

    #[inline]
    fn base_height(&self) -> isize {
        self.size.height
    }

    #[inline]
    fn preferred_line_height(&self) -> isize {
        self.line_height
    }

    #[inline]
    fn width_of(&self, character: char) -> isize {
        if let Some(width_table) = self.width_table {
            let c = character as usize;
            if c >= 0x20 && c < 0x80 {
                width_table[c - 0x20] as isize
            } else {
                self.width()
            }
        } else {
            self.width()
        }
    }

    #[inline]
    fn height_of(&self, character: char) -> isize {
        let _ = character;
        self.size.height
    }

    fn draw_char(
        &self,
        character: char,
        bitmap: &mut Bitmap,
        origin: Point,
        _height: isize,
        color: AmbiguousColor,
    ) {
        if let Some(font) = self.glyph_for(character) {
            let origin = Point::new(origin.x, origin.y + self.leading);
            let size = Size::new(self.width_of(character), self.size.height());
            bitmap.draw_font(font, size, origin, color);
        }
    }
}
