// A Window System

use crate::fonts::*;
use crate::graphics::bitmap::*;
use crate::graphics::color::*;
use crate::graphics::coords::*;
use crate::sync::atomicflags::AtomicBitflags;
use crate::{io::hid::*, system::System};
use alloc::boxed::Box;
use bitflags::*;
use core::cell::UnsafeCell;
use core::num::NonZeroUsize;

static mut WM: Option<Box<WindowManager>> = None;

const WINDOW_TITLE_LENGTH: usize = 32;

const WINDOW_BORDER_PADDING: isize = 1;
const WINDOW_TITLE_HEIGHT: isize = 24;

const WINDOW_BORDER_COLOR: IndexedColor = IndexedColor::from_rgb(0x333333);
const WINDOW_DEFAULT_BGCOLOR: IndexedColor = IndexedColor::WHITE;
const WINDOW_ACTIVE_TITLE_BG_COLOR: IndexedColor = IndexedColor::from_rgb(0xCCCCCC);
const WINDOW_ACTIVE_TITLE_FG_COLOR: IndexedColor = IndexedColor::from_rgb(0x333333);
const WINDOW_INACTIVE_TITLE_BG_COLOR: IndexedColor = IndexedColor::from_rgb(0xFFFFFF);
const WINDOW_INACTIVE_TITLE_FG_COLOR: IndexedColor = IndexedColor::from_rgb(0x999999);

const MOUSE_POINTER_WIDTH: usize = 12;
const MOUSE_POINTER_HEIGHT: usize = 20;
const MOUSE_POINTER_COLOR_KEY: IndexedColor = IndexedColor(0xFF);
const MOUSE_POINTER_SOURCE: [u8; MOUSE_POINTER_WIDTH * MOUSE_POINTER_HEIGHT] = [
    0x0F, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x0F, 0x0F, 0xFF, 0xFF,
    0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x0F, 0x07, 0x0F, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
    0xFF, 0xFF, 0xFF, 0xFF, 0x0F, 0x00, 0x07, 0x0F, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
    0x0F, 0x00, 0x00, 0x07, 0x0F, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x0F, 0x00, 0x00, 0x00,
    0x07, 0x0F, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x0F, 0x00, 0x00, 0x00, 0x00, 0x07, 0x0F, 0xFF,
    0xFF, 0xFF, 0xFF, 0xFF, 0x0F, 0x00, 0x00, 0x00, 0x00, 0x00, 0x07, 0x0F, 0xFF, 0xFF, 0xFF, 0xFF,
    0x0F, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x07, 0x0F, 0xFF, 0xFF, 0xFF, 0x0F, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x07, 0x0F, 0xFF, 0xFF, 0x0F, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x07, 0x0F, 0xFF, 0x0F, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x07, 0x0F,
    0x0F, 0x00, 0x00, 0x00, 0x00, 0x00, 0x07, 0x0F, 0x0F, 0x0F, 0x0F, 0x0F, 0x0F, 0x00, 0x00, 0x07,
    0x0F, 0x07, 0x00, 0x0F, 0xFF, 0xFF, 0xFF, 0xFF, 0x0F, 0x00, 0x07, 0x0F, 0xFF, 0x0F, 0x00, 0x07,
    0x0F, 0xFF, 0xFF, 0xFF, 0x0F, 0x07, 0x0F, 0xFF, 0xFF, 0x0F, 0x07, 0x00, 0x0F, 0xFF, 0xFF, 0xFF,
    0x0F, 0x0F, 0xFF, 0xFF, 0xFF, 0xFF, 0x0F, 0x00, 0x07, 0x0F, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
    0xFF, 0xFF, 0x0F, 0x07, 0x00, 0x0F, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x0F,
    0x0F, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
];

pub struct WindowManager {
    last_key: Option<char>,
    pointer_x: isize,
    pointer_y: isize,

    main_screen: &'static mut OsMutBitmap8<'static>,
    screen_insets: EdgeInsets,

    active: Option<WindowHandle>,
}

impl WindowManager {
    fn new(main_screen: &'static mut OsMutBitmap8) -> Box<Self> {
        Box::new(Self {
            last_key: None,
            pointer_x: 320,
            pointer_y: 240,
            screen_insets: EdgeInsets::padding_each(0),
            main_screen,
            active: None,
        })
    }

    pub(crate) unsafe fn init() {
        WM = Some(WindowManager::new(System::main_screen()));
    }

    #[inline]
    #[track_caller]
    fn shared() -> &'static mut Self {
        unsafe { WM.as_mut().unwrap() }
    }

    #[inline]
    fn shared_opt() -> Option<&'static mut Box<Self>> {
        unsafe { WM.as_mut() }
    }

    fn next_window_handle() -> WindowHandle {
        // static NEXT_ID: AtomicUsize = AtomicUsize::new(1);
        // WindowHandle::new(NEXT_ID.fetch_add(1, Ordering::SeqCst)).unwrap()
        WindowHandle::new(1).unwrap()
    }

    #[inline]
    pub fn user_screen_bounds() -> Rect {
        let shared = Self::shared();
        shared.main_screen.bounds().insets_by(shared.screen_insets)
    }

    #[inline]
    pub fn screen_insets() -> EdgeInsets {
        let shared = Self::shared();
        shared.screen_insets
    }

    #[inline]
    pub fn add_screen_insets(insets: EdgeInsets) {
        let shared = Self::shared();
        shared.screen_insets += insets;
    }

    #[inline]
    pub fn main_screen<'a>(&self) -> &'a mut OsMutBitmap8<'static> {
        let shared = Self::shared();
        &mut shared.main_screen
    }

    pub fn post_key_event(event: KeyEvent) {
        let shared = match Self::shared_opt() {
            Some(v) => v,
            None => return,
        };
        if event.usage() == Usage::DELETE
            && event.modifier().has_ctrl()
            && event.modifier().has_alt()
        {
            unsafe {
                System::reset();
            }
        } else {
            if let Some(event) = event.key_data() {
                shared.last_key = Some(event.into_char());
            }
        }
    }

    pub fn post_mouse_event(mouse_state: &mut MouseState) {
        let shared = match Self::shared_opt() {
            Some(v) => v,
            None => return,
        };

        let screen = System::main_screen();

        let mut pointer = Point::new(0, 0);
        core::mem::swap(&mut mouse_state.x, &mut pointer.x);
        core::mem::swap(&mut mouse_state.y, &mut pointer.y);
        let button_changes: MouseButton = mouse_state.current_buttons ^ mouse_state.prev_buttons;
        let button_down: MouseButton = button_changes & mouse_state.current_buttons;
        let button_up: MouseButton = button_changes & mouse_state.prev_buttons;
        let button_changed = !button_changes.is_empty();

        let mut x = shared.pointer_x + pointer.x;
        let mut y = shared.pointer_y + pointer.y;
        if x < 0 {
            x = 0;
        } else if x >= screen.width() as isize {
            x = screen.width() as isize - 1;
        }
        if y < 0 {
            y = 0;
        } else if y >= screen.height() as isize {
            y = screen.height() as isize - 1;
        }
        shared.pointer_x = x;
        shared.pointer_y = y;

        let origin = Point::new(x, y);
        let cursor = OsBitmap8::from_bytes(
            &MOUSE_POINTER_SOURCE,
            Size::new(MOUSE_POINTER_WIDTH as isize, MOUSE_POINTER_HEIGHT as isize),
        );
        screen.blt_with_key(&cursor, origin, cursor.bounds(), MOUSE_POINTER_COLOR_KEY);
    }

    pub fn get_key() -> Option<char> {
        let shared = Self::shared();
        core::mem::replace(&mut shared.last_key, None)
    }
}

/// Raw implementation of the window
#[allow(dead_code)]
pub struct RawWindow {
    /// Refer to the self owned handle
    handle: WindowHandle,

    // Properties
    attributes: AtomicBitflags<WindowAttributes>,
    style: WindowStyle,
    level: WindowLevel,

    // Placement and Size
    frame: Rect,
    content_insets: EdgeInsets,

    // Appearances
    bg_color: IndexedColor,
    bitmap: Option<Box<UnsafeCell<BoxedBitmap8>>>,

    /// Window Title
    title: [u8; WINDOW_TITLE_LENGTH],
    // Messages and Events
    // queue: Option<ArrayQueue<WindowMessage>>,
}

bitflags! {
    pub struct WindowStyle: u8 {
        const BORDER        = 0b0000_0001;
        const TITLE         = 0b0000_0010;
        const NAKED         = 0b0000_0100;
        const OPAQUE        = 0b0000_1000;
        const PINCHABLE     = 0b0001_0000;
        const FLOATING      = 0b0010_0000;

        const DEFAULT = Self::BORDER.bits | Self::TITLE.bits;
    }
}

impl WindowStyle {
    fn as_content_insets(self) -> EdgeInsets {
        let mut insets = if self.contains(Self::BORDER) {
            EdgeInsets::padding_each(WINDOW_BORDER_PADDING)
        } else {
            EdgeInsets::padding_each(0)
        };
        if self.contains(Self::TITLE) {
            insets.top += WINDOW_TITLE_HEIGHT;
        }
        insets
    }
}

bitflags! {
    struct WindowAttributes: usize {
        const NEEDS_REDRAW  = 0b0000_0001;
        const VISIBLE       = 0b0000_0010;
    }
}

impl Into<usize> for WindowAttributes {
    fn into(self) -> usize {
        self.bits()
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct WindowLevel(pub u8);

impl WindowLevel {
    pub const ROOT: WindowLevel = WindowLevel(0);
    pub const DESKTOP_ITEMS: WindowLevel = WindowLevel(1);
    pub const NORMAL: WindowLevel = WindowLevel(32);
    pub const FLOATING: WindowLevel = WindowLevel(64);
    pub const POPUP_BARRIER: WindowLevel = WindowLevel(96);
    pub const POPUP: WindowLevel = WindowLevel(97);
    pub const POINTER: WindowLevel = WindowLevel(127);
}

impl RawWindow {
    #[inline]
    pub fn bounds(&self) -> Rect {
        self.frame.size().into()
    }

    fn bitmap<'a>(&self) -> Option<OsMutBitmap8<'a>> {
        match self.bitmap.as_ref() {
            Some(v) => {
                let q = unsafe { v.as_ref().get().as_mut() };
                q.map(|v| OsMutBitmap8::from(v))
            }
            None => None,
        }
    }

    fn title_frame(&self) -> Rect {
        if self.style.contains(WindowStyle::TITLE) {
            Rect::new(
                WINDOW_BORDER_PADDING,
                WINDOW_BORDER_PADDING,
                self.frame.width() - WINDOW_BORDER_PADDING * 2,
                WINDOW_TITLE_HEIGHT - WINDOW_BORDER_PADDING - 1,
            )
        } else {
            Rect::new(0, 0, 0, 0)
        }
    }

    pub fn draw_frame(&mut self) {
        if let Some(mut bitmap) = self.bitmap() {
            let is_active = true; //self.is_active();

            if self.style.contains(WindowStyle::BORDER) {
                bitmap.draw_rect(self.frame.size().into(), WINDOW_BORDER_COLOR);
            }
            if self.style.contains(WindowStyle::TITLE) {
                let title_rect = self.title_frame();
                bitmap.fill_rect(
                    title_rect,
                    if is_active {
                        WINDOW_ACTIVE_TITLE_BG_COLOR
                    } else {
                        WINDOW_INACTIVE_TITLE_BG_COLOR
                    },
                );
                bitmap.draw_hline(
                    Point::new(WINDOW_BORDER_PADDING, WINDOW_TITLE_HEIGHT - 1),
                    title_rect.width(),
                    WINDOW_BORDER_COLOR,
                );
                let font = FontManager::fixed_system_font();

                if let Some(s) = self.title() {
                    font.write_str(
                        s,
                        &mut bitmap,
                        Point::new(8, 4),
                        if is_active {
                            WINDOW_ACTIVE_TITLE_FG_COLOR
                        } else {
                            WINDOW_INACTIVE_TITLE_FG_COLOR
                        },
                    );
                }
            }
        }
    }

    pub fn draw_in_rect<F>(&self, rect: Rect, f: F) -> Result<(), WindowDrawingError>
    where
        F: FnOnce(&mut OsMutBitmap8) -> (),
    {
        let window = self;
        let mut bitmap = match window.bitmap() {
            Some(bitmap) => bitmap,
            None => return Err(WindowDrawingError::NoBitmap),
        };
        let bounds = Rect::from(window.frame.size).insets_by(window.content_insets);
        let origin = Point::new(isize::max(0, rect.x()), isize::max(0, rect.y()));
        let coords = match Coordinates::from_rect(Rect::new(
            origin.x + bounds.x(),
            origin.y + bounds.y(),
            isize::min(rect.width(), bounds.width() - origin.x),
            isize::min(rect.height(), bounds.height() - origin.y),
        )) {
            Ok(coords) => coords,
            Err(_) => return Err(WindowDrawingError::InconsistentCoordinates),
        };
        if coords.left > coords.right || coords.top > coords.bottom {
            return Err(WindowDrawingError::InconsistentCoordinates);
        }

        let rect = coords.into();
        match bitmap.view(rect, |bitmap| f(bitmap)) {
            Some(_) => Ok(()),
            None => Err(WindowDrawingError::InconsistentCoordinates),
        }
    }

    pub fn draw_to_screen(&mut self, rect: Rect) {
        let mut frame = rect;
        frame.origin += self.frame.origin;
        let main_screen = WindowManager::shared().main_screen();
        // let off_screen = WindowManager::shared().off_screen.as_ref();
        // if self.draw_into(&mut off_screen, frame) {
        //     main_screen.blt(off_screen, frame.origin, frame);
        // }
        self.draw_into(main_screen, frame);
    }

    fn draw_into(&mut self, target_bitmap: &mut OsMutBitmap8, frame: Rect) -> bool {
        let coords1 = match Coordinates::from_rect(frame) {
            Ok(v) => v,
            Err(_) => return false,
        };
        if let Some(mut bitmap) = self.bitmap() {
            let origin = self.frame.origin;
            let rect = Rect::from(self.frame.size);
            target_bitmap.blt(&mut bitmap, origin, rect);
        }
        true
    }

    #[inline]
    fn is_active(&self) -> bool {
        WindowManager::shared().active.contains(&self.handle)
    }

    fn set_title_array(array: &mut [u8; WINDOW_TITLE_LENGTH], title: &str) {
        let mut i = 1;
        for c in title.bytes() {
            if i >= WINDOW_TITLE_LENGTH {
                break;
            }
            array[i] = c;
            i += 1;
        }
        array[0] = i as u8 - 1;
    }

    fn set_title(&mut self, title: &str) {
        RawWindow::set_title_array(&mut self.title, title);
        self.refresh_title();
    }

    #[inline]
    fn refresh_title(&mut self) {
        self.draw_frame();
        if self.style.contains(WindowStyle::TITLE) {
            // self.invalidate_rect(self.title_frame());
        }
    }

    fn title<'a>(&self) -> Option<&'a str> {
        let len = self.title[0] as usize;
        match len {
            0 => None,
            _ => core::str::from_utf8(unsafe { core::slice::from_raw_parts(&self.title[1], len) })
                .ok(),
        }
    }
}

pub struct WindowBuilder {
    frame: Rect,
    style: WindowStyle,
    level: WindowLevel,
    bg_color: IndexedColor,
    title: [u8; WINDOW_TITLE_LENGTH],
    queue_size: usize,
    no_bitmap: bool,
}

impl WindowBuilder {
    pub fn new(title: &str) -> Self {
        let window = Self {
            frame: Rect::new(isize::MIN, isize::MIN, 300, 300),
            level: WindowLevel::NORMAL,
            style: WindowStyle::DEFAULT,
            bg_color: WINDOW_DEFAULT_BGCOLOR,
            title: [0; WINDOW_TITLE_LENGTH],
            queue_size: 100,
            no_bitmap: false,
        };
        window.title(title).style(WindowStyle::DEFAULT)
    }

    #[inline]
    pub fn build(self) -> WindowHandle {
        self.build_inner().handle
    }

    // TODO:
    pub fn build_inner(mut self) -> Box<RawWindow> {
        let screen_bounds = WindowManager::user_screen_bounds();
        let window_insets = self.style.as_content_insets();
        let content_insets = window_insets;
        let mut frame = self.frame;
        if self.style.contains(WindowStyle::NAKED) {
            frame.size += window_insets;
        }
        if frame.x() == isize::MIN {
            frame.origin.x = (screen_bounds.width() - frame.width()) / 2;
        } else if frame.x() < 0 {
            frame.origin.x += screen_bounds.x() + screen_bounds.width();
        }
        if frame.y() == isize::MIN {
            frame.origin.y = isize::max(
                screen_bounds.y(),
                (screen_bounds.height() - frame.height()) / 2,
            );
        } else if frame.y() < 0 {
            frame.origin.y += screen_bounds.y() + screen_bounds.height();
        }

        if self.style.contains(WindowStyle::FLOATING) {
            self.level = WindowLevel::FLOATING;
        }

        let attributes = if self.level == WindowLevel::ROOT {
            AtomicBitflags::new(WindowAttributes::VISIBLE)
        } else {
            AtomicBitflags::empty()
        };

        // let queue = match self.queue_size {
        //     0 => None,
        //     _ => Some(ArrayQueue::new(self.queue_size)),
        // };

        let handle = WindowManager::next_window_handle();
        let mut window = Box::new(RawWindow {
            handle,
            frame,
            content_insets,
            style: self.style,
            level: self.level,
            bg_color: self.bg_color,
            bitmap: None,
            title: self.title,
            attributes,
        });

        if !self.no_bitmap {
            window.bitmap = Some(Box::new(UnsafeCell::new(BoxedBitmap8::new(
                frame.size(),
                self.bg_color,
            ))));
        };

        window
    }

    #[inline]
    pub fn style(mut self, style: WindowStyle) -> Self {
        self.style = style;
        self
    }

    #[inline]
    pub fn style_add(mut self, style: WindowStyle) -> Self {
        self.style |= style;
        self
    }

    #[inline]
    pub fn title(mut self, title: &str) -> Self {
        RawWindow::set_title_array(&mut self.title, title);
        self
    }

    #[inline]
    pub const fn level(mut self, level: WindowLevel) -> Self {
        self.level = level;
        self
    }

    #[inline]
    pub const fn frame(mut self, frame: Rect) -> Self {
        self.frame = frame;
        self
    }

    #[inline]
    pub const fn center(mut self) -> Self {
        self.frame.origin = Point::new(isize::MIN, isize::MIN);
        self
    }

    #[inline]
    pub const fn origin(mut self, origin: Point) -> Self {
        self.frame.origin = origin;
        self
    }

    #[inline]
    pub const fn size(mut self, size: Size) -> Self {
        self.frame.size = size;
        self
    }

    #[inline]
    pub const fn bg_color(mut self, bg_color: IndexedColor) -> Self {
        self.bg_color = bg_color;
        self
    }

    #[inline]
    pub const fn message_queue_size(mut self, queue_size: usize) -> Self {
        self.queue_size = queue_size;
        self
    }

    #[inline]
    pub const fn without_message_queue(mut self) -> Self {
        self.queue_size = 0;
        self
    }

    #[inline]
    pub const fn without_bitmap(mut self) -> Self {
        self.no_bitmap = true;
        self
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct WindowHandle(NonZeroUsize);

impl WindowHandle {
    #[inline]
    fn new(val: usize) -> Option<Self> {
        NonZeroUsize::new(val).map(|x| Self(x))
    }

    #[inline]
    pub const fn as_usize(&self) -> usize {
        self.0.get()
    }
}

#[non_exhaustive]
#[derive(Debug, Copy, Clone)]
pub enum WindowDrawingError {
    NoBitmap,
    InconsistentCoordinates,
}

#[non_exhaustive]
#[derive(Debug, Copy, Clone)]
pub enum WindowPostError {
    NotFound,
    Full,
}
