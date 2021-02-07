// A Window System

use crate::graphics::bitmap::*;
use crate::graphics::color::*;
use crate::graphics::coords::*;
use crate::sync::atomicflags::AtomicBitflags;
use crate::*;
use crate::{arch::cpu::Cpu, fonts::*};
use crate::{io::hid::*, system::System};
use alloc::boxed::Box;
use alloc::collections::btree_map::BTreeMap;
use alloc::vec::Vec;
use bitflags::*;
use core::cell::UnsafeCell;
use core::cmp;
// use core::fmt::Write;
use core::num::NonZeroUsize;

static mut WM: Option<Box<WindowManager>> = None;

const MAX_WINDOWS: usize = 255;

const WINDOW_TITLE_LENGTH: usize = 32;

const WINDOW_BORDER_PADDING: isize = 1;
const WINDOW_TITLE_HEIGHT: isize = 24;

const WINDOW_BORDER_COLOR: IndexedColor = IndexedColor::from_rgb(0x333333);
const WINDOW_DEFAULT_BGCOLOR: IndexedColor = IndexedColor::WHITE;
const WINDOW_DEFAULT_KEY_COLOR: IndexedColor = IndexedColor::DEFAULT_KEY;
const WINDOW_ACTIVE_TITLE_BG_COLOR: IndexedColor = IndexedColor::from_rgb(0xCCCCCC);
const WINDOW_ACTIVE_TITLE_FG_COLOR: IndexedColor = IndexedColor::from_rgb(0x333333);
const WINDOW_INACTIVE_TITLE_BG_COLOR: IndexedColor = IndexedColor::from_rgb(0xFFFFFF);
const WINDOW_INACTIVE_TITLE_FG_COLOR: IndexedColor = IndexedColor::from_rgb(0x999999);

const MOUSE_POINTER_WIDTH: usize = 12;
const MOUSE_POINTER_HEIGHT: usize = 20;
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

    main_screen: &'static mut Bitmap8<'static>,
    screen_insets: EdgeInsets,

    window_pool: BTreeMap<WindowHandle, Box<RawWindow>>,
    window_orders: Vec<WindowHandle>,

    active: Option<WindowHandle>,
    captured: Option<WindowHandle>,
    root: WindowHandle,
    pointer: WindowHandle,
}

impl WindowManager {
    pub(crate) unsafe fn init() {
        let main_screen = System::main_screen();
        let pointer_x = main_screen.width() as isize / 2;
        let pointer_y = main_screen.height() as isize / 2;

        let mut window_pool = BTreeMap::new();
        let mut window_orders = Vec::with_capacity(MAX_WINDOWS);

        let root = {
            let window = WindowBuilder::new("Root")
                .style(WindowStyle::NAKED)
                .level(WindowLevel::ROOT)
                .size(main_screen.size())
                .without_bitmap()
                .without_message_queue()
                .build_inner();

            let handle = window.handle;
            window_pool.insert(handle, window);
            handle
        };
        window_orders.push(root);

        let pointer = {
            let pointer_size =
                Size::new(MOUSE_POINTER_WIDTH as isize, MOUSE_POINTER_HEIGHT as isize);
            let window = WindowBuilder::new("Root")
                .style(WindowStyle::NAKED | WindowStyle::TRANSPARENT)
                .level(WindowLevel::POINTER)
                .size(pointer_size)
                .without_message_queue()
                .build_inner();

            window
                .draw_in_rect(pointer_size.into(), |bitmap| {
                    let cursor = ConstBitmap8::from_bytes(&MOUSE_POINTER_SOURCE, pointer_size);
                    bitmap.blt(&cursor, Point::new(0, 0), pointer_size.into());
                })
                .unwrap();

            let handle = window.handle;
            window_pool.insert(handle, window);
            handle
        };
        window_orders.push(pointer);

        WM = Some(Box::new(Self {
            last_key: None,
            pointer_x,
            pointer_y,
            screen_insets: EdgeInsets::default(),
            main_screen,
            window_pool,
            window_orders,
            active: None,
            captured: None,
            root,
            pointer,
        }));
    }

    #[inline]
    pub fn is_enabled() -> bool {
        unsafe { WM.is_some() }
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

    fn add(window: Box<RawWindow>) {
        unsafe {
            Cpu::without_interrupts(|| {
                let shared = Self::shared();
                shared.window_pool.insert(window.handle, window);
            })
        }
    }

    #[allow(dead_code)]
    fn remove(_window: &WindowHandle) {
        // TODO:
    }

    #[inline]
    fn get(&self, key: &WindowHandle) -> Option<&Box<RawWindow>> {
        unsafe { Cpu::without_interrupts(|| self.window_pool.get(key)) }
    }

    #[inline]
    fn get_mut(&mut self, key: &WindowHandle) -> Option<&mut Box<RawWindow>> {
        unsafe { Cpu::without_interrupts(move || self.window_pool.get_mut(key)) }
    }

    /// SAFETY: MUST lock window_orders
    unsafe fn add_hierarchy(window: WindowHandle) {
        let window = match window.get() {
            Some(v) => v,
            None => return,
        };

        let shared = WindowManager::shared();
        Self::remove_hierarchy(window.handle);

        let mut insert_position = None;
        for (index, lhs) in shared.window_orders.iter().enumerate() {
            if lhs.as_ref().level > window.level {
                insert_position = Some(index);
                break;
            }
        }
        if let Some(insert_position) = insert_position {
            shared.window_orders.insert(insert_position, window.handle);
        } else {
            shared.window_orders.push(window.handle);
        }

        window.attributes.insert(WindowAttributes::VISIBLE);
    }

    /// SAFETY: MUST lock window_orders
    unsafe fn remove_hierarchy(window: WindowHandle) {
        let window = match window.get() {
            Some(v) => v,
            None => return,
        };

        window.attributes.remove(WindowAttributes::VISIBLE);

        let shared = WindowManager::shared();
        let mut remove_position = None;
        for (index, lhs) in shared.window_orders.iter().enumerate() {
            if *lhs == window.handle {
                remove_position = Some(index);
                break;
            }
        }
        if let Some(remove_position) = remove_position {
            shared.window_orders.remove(remove_position);
        }
    }

    #[inline]
    pub fn main_screen_bounds() -> Rect {
        match Self::shared_opt() {
            Some(shared) => shared.main_screen.bounds(),
            None => System::main_screen().size().into(),
        }
    }

    #[inline]
    pub fn user_screen_bounds() -> Rect {
        match Self::shared_opt() {
            Some(shared) => shared.main_screen.bounds().insets_by(shared.screen_insets),
            None => System::main_screen().size().into(),
        }
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
    pub fn main_screen<'a>(&self) -> &'a mut Bitmap8<'static> {
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

        shared.pointer.update(|pointer| {
            let mut frame = pointer.frame;
            frame.origin = Point::new(x, y);
            pointer.set_frame(frame);
        });
    }

    pub fn get_key() -> Option<char> {
        let shared = Self::shared();
        core::mem::replace(&mut shared.last_key, None)
    }

    #[inline]
    pub fn invalidate_screen(rect: Rect) {
        let shared = Self::shared();
        let _ = shared.root.update_opt(|root| {
            root.invalidate_rect(rect);
        });
    }

    pub fn set_desktop_color(color: IndexedColor) {
        let shared = Self::shared();
        let _ = shared.root.update_opt(|root| {
            root.bitmap = None;
            root.set_bg_color(color);
        });
    }

    pub fn set_desktop_bitmap(bitmap: Option<Box<UnsafeCell<VecBitmap8>>>) {
        let shared = Self::shared();
        let _ = shared.root.update_opt(|root| {
            root.bitmap = bitmap;
            root.set_needs_display();
        });
    }

    #[inline]
    pub fn is_pointer_visible() -> bool {
        Self::shared()
            .pointer
            .get()
            .map(|v| v.is_visible())
            .unwrap_or(false)
    }

    pub fn set_pointer_visible(visible: bool) -> bool {
        Self::shared()
            .pointer
            .update_opt(|pointer| {
                let result = pointer.is_visible();
                if visible {
                    pointer.show();
                } else if result {
                    pointer.hide();
                }
                result
            })
            .unwrap_or(false)
    }

    #[inline]
    pub fn while_hiding_pointer<F, R>(f: F) -> R
    where
        F: FnOnce() -> R,
    {
        let pointer_visible = Self::set_pointer_visible(false);
        let result = f();
        Self::set_pointer_visible(pointer_visible);
        result
    }

    pub fn save_screen_to(bitmap: &mut Bitmap8, rect: Rect) {
        let shared = Self::shared();
        Self::while_hiding_pointer(|| shared.root.update(|v| v.draw_into(bitmap, rect)));
    }

    fn set_active(window: Option<WindowHandle>) {
        let shared = Self::shared();
        if let Some(old_active) = shared.active {
            shared.active = window;
            let _ = old_active.update_opt(|window| window.refresh_title());
            if let Some(active) = window {
                active.show();
            }
        } else {
            shared.active = window;
            if let Some(active) = window {
                active.show();
            }
        }
    }
}

/// Raw implementation of the window
#[allow(dead_code)]
struct RawWindow {
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
    key_color: IndexedColor,
    bitmap: Option<Box<UnsafeCell<VecBitmap8>>>,

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
        const TRANSPARENT   = 0b0100_0000;

        const DEFAULT = Self::BORDER.bits | Self::TITLE.bits;
    }
}

impl WindowStyle {
    fn as_content_insets(self) -> EdgeInsets {
        let mut insets = if self.contains(Self::BORDER) {
            EdgeInsets::padding_each(WINDOW_BORDER_PADDING)
        } else {
            EdgeInsets::default()
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
    fn actual_bounds(&self) -> Rect {
        self.frame.size().into()
    }

    #[inline]
    fn is_visible(&self) -> bool {
        self.attributes.contains(WindowAttributes::VISIBLE)
    }

    #[inline]
    fn is_active(&self) -> bool {
        WindowManager::shared().active.contains(&self.handle)
    }

    fn show(&mut self) {
        self.draw_frame();
        unsafe {
            Cpu::without_interrupts(|| {
                WindowManager::add_hierarchy(self.handle);
            })
        }
        // WindowManager::invalidate_screen(window.frame);
        self.set_needs_display();
    }

    fn hide(&self) {
        let shared = WindowManager::shared();
        let frame = self.frame;
        // let new_active = if shared.active.contains(self.handle) {
        //     self.prev()
        // } else {
        //     None
        // };
        if shared.captured.contains(&self.handle) {
            shared.captured = None;
        }
        unsafe {
            Cpu::without_interrupts(|| {
                WindowManager::remove_hierarchy(self.handle);
            });
        }
        WindowManager::invalidate_screen(frame);
        // if new_active.is_some() {
        //     WindowManager::set_active(new_active);
        // }
    }

    fn set_frame(&mut self, new_frame: Rect) {
        let old_frame = self.frame;
        if old_frame != new_frame {
            // let sized = old_frame.size() != new_frame.size();
            self.frame = new_frame;
            if self.attributes.contains(WindowAttributes::VISIBLE) {
                WindowManager::invalidate_screen(old_frame);
                self.draw_frame();
                self.set_needs_display();
            }
        }
    }

    fn set_bg_color(&mut self, color: IndexedColor) {
        self.bg_color = color;
        if let Some(mut bitmap) = self.bitmap() {
            bitmap.fill_rect(bitmap.bounds(), color);
            self.draw_frame();
        }
        self.set_needs_display();
    }

    fn bitmap<'a>(&self) -> Option<Bitmap8<'a>> {
        match self.bitmap.as_ref() {
            Some(v) => {
                let q = unsafe { v.as_ref().get().as_mut() };
                q.map(|v| Bitmap8::from(v))
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

    fn draw_frame(&mut self) {
        if let Some(mut bitmap) = self.bitmap() {
            let is_active = self.is_active();

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

    fn draw_in_rect<F>(&self, rect: Rect, f: F) -> Result<(), WindowDrawingError>
    where
        F: FnOnce(&mut Bitmap8) -> (),
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

    fn draw_to_screen(&self, rect: Rect) {
        let mut frame = rect;
        frame.origin += self.frame.origin;
        let main_screen = WindowManager::shared().main_screen();
        // let off_screen = WindowManager::shared().off_screen.as_ref();
        // if self.draw_into(&mut off_screen, frame) {
        //     main_screen.blt(off_screen, frame.origin, frame);
        // }
        self.draw_into(main_screen, frame);
    }

    fn draw_into(&self, target_bitmap: &mut Bitmap8, frame: Rect) -> bool {
        let coords1 = match Coordinates::from_rect(frame) {
            Ok(v) => v,
            Err(_) => return false,
        };

        let shared = WindowManager::shared();

        for handle in shared.window_orders.iter() {
            handle.update(|window| {
                let coords2 = match Coordinates::from_rect(window.frame) {
                    Ok(v) => v,
                    Err(_) => return,
                };
                if frame.is_within_rect(window.frame) {
                    let blt_origin = Point::new(
                        cmp::max(coords1.left, coords2.left),
                        cmp::max(coords1.top, coords2.top),
                    );
                    let x = if coords1.left > coords2.left {
                        coords1.left - coords2.left
                    } else {
                        0
                    };
                    let y = if coords1.top > coords2.top {
                        coords1.top - coords2.top
                    } else {
                        0
                    };
                    let blt_rect = Rect::new(
                        x,
                        y,
                        cmp::min(coords1.right, coords2.right)
                            - cmp::max(coords1.left, coords2.left),
                        cmp::min(coords1.bottom, coords2.bottom)
                            - cmp::max(coords1.top, coords2.top),
                    );

                    if let Some(mut bitmap) = window.bitmap() {
                        if window.style.contains(WindowStyle::TRANSPARENT) {
                            target_bitmap.blt_with_key(
                                &mut bitmap,
                                blt_origin,
                                blt_rect,
                                window.key_color,
                            );
                        } else {
                            target_bitmap.blt(&mut bitmap, blt_origin, blt_rect);
                        }
                    } else {
                        target_bitmap.fill_rect(blt_rect, window.bg_color);
                    }
                }
            })
        }

        true
    }

    #[inline]
    fn set_needs_display(&mut self) {
        // TODO:
        self.invalidate_rect(self.actual_bounds());
    }

    #[inline]
    fn invalidate_rect(&mut self, rect: Rect) {
        if self.attributes.contains(WindowAttributes::VISIBLE) {
            self.draw_to_screen(rect);
        }
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
    key_color: IndexedColor,
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
            key_color: WINDOW_DEFAULT_KEY_COLOR,
            title: [0; WINDOW_TITLE_LENGTH],
            queue_size: 100,
            no_bitmap: false,
        };
        window.title(title).style(WindowStyle::DEFAULT)
    }

    #[inline]
    pub fn build(self) -> WindowHandle {
        let window = self.build_inner();
        let handle = window.handle;
        WindowManager::add(window);
        handle
    }

    fn build_inner(mut self) -> Box<RawWindow> {
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

        let handle = WindowHandle::next();
        let mut window = Box::new(RawWindow {
            handle,
            frame,
            content_insets,
            style: self.style,
            level: self.level,
            bg_color: self.bg_color,
            key_color: self.key_color,
            bitmap: None,
            title: self.title,
            attributes,
        });

        if !self.no_bitmap {
            window.bitmap = Some(Box::new(UnsafeCell::new(VecBitmap8::new(
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
    pub const fn key_color(mut self, key_color: IndexedColor) -> Self {
        self.key_color = key_color;
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

    /// Acquire the next window handle
    #[inline]
    fn next() -> Self {
        static mut NEXT_ID: usize = 1;
        Self::new(unsafe { Cpu::interlocked_increment(&mut NEXT_ID) }).unwrap()
    }

    #[inline]
    fn get<'a>(&self) -> Option<&'a Box<RawWindow>> {
        let shared = WindowManager::shared();
        shared.get(self)
    }

    #[inline]
    #[track_caller]
    fn as_ref<'a>(&self) -> &'a RawWindow {
        self.get().unwrap()
    }

    #[inline]
    fn update_opt<F, R>(&self, f: F) -> Option<R>
    where
        F: FnOnce(&mut RawWindow) -> R,
    {
        let shared = WindowManager::shared();
        let window = shared.get_mut(self);
        window.map(|v| f(v))
    }

    #[inline]
    #[track_caller]
    fn update<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut RawWindow) -> R,
    {
        self.update_opt(f).unwrap()
    }

    // :-:-:-:-:

    #[inline]
    pub fn set_title(&self, title: &str) {
        self.update(|window| {
            window.set_title(title);
        });
    }

    #[inline]
    pub fn title<'a>(&self) -> Option<&'a str> {
        self.get().and_then(|v| v.title())
    }

    pub fn set_bg_color(&self, color: IndexedColor) {
        self.update(|window| {
            window.set_bg_color(color);
        });
    }

    #[inline]
    pub fn bg_color(&self) -> IndexedColor {
        self.as_ref().bg_color
    }

    #[inline]
    pub fn frame(&self) -> Rect {
        self.as_ref().frame
    }

    pub fn set_frame(&self, rect: Rect) {
        self.update(|window| {
            window.set_frame(rect);
        });
    }

    #[inline]
    pub fn bounds(&self) -> Rect {
        self.as_ref().frame.size().into()
    }

    #[inline]
    pub fn content_insets(&self) -> EdgeInsets {
        self.as_ref().content_insets
    }

    #[inline]
    pub fn move_by(&self, delta: Point) {
        let mut new_rect = self.frame();
        new_rect.origin += delta;
        self.set_frame(new_rect);
    }

    #[inline]
    pub fn move_to(&self, new_origin: Point) {
        let mut new_rect = self.frame();
        new_rect.origin = new_origin;
        self.set_frame(new_rect);
    }

    #[inline]
    pub fn resize_to(&self, new_size: Size) {
        let mut new_rect = self.frame();
        new_rect.size = new_size;
        self.set_frame(new_rect);
    }

    #[inline]
    pub fn show(&self) {
        self.update(|window| window.show());
    }

    #[inline]
    pub fn hide(&self) {
        self.update(|window| window.hide());
    }

    #[inline]
    pub fn close(&self) {
        // TODO: remove window
        self.hide();
    }

    #[inline]
    pub fn is_visible(&self) -> bool {
        self.as_ref().attributes.contains(WindowAttributes::VISIBLE)
    }

    #[inline]
    pub fn make_active(&self) {
        WindowManager::set_active(Some(*self));
    }

    #[inline]
    pub fn invalidate_rect(&self, rect: Rect) {
        self.update(|window| window.invalidate_rect(rect));
    }

    #[inline]
    pub fn set_needs_display(&self) {
        self.update(|window| window.set_needs_display());
    }

    #[inline]
    pub fn draw<F>(&self, f: F) -> Result<(), WindowDrawingError>
    where
        F: FnOnce(&mut Bitmap8) -> (),
    {
        self.update(|window| {
            let rect = window.actual_bounds().insets_by(window.content_insets);
            self.draw_in_rect(rect.size().into(), f).map(|_| {
                window.invalidate_rect(rect);
            })
        })
    }

    pub fn draw_in_rect<F>(&self, rect: Rect, f: F) -> Result<(), WindowDrawingError>
    where
        F: FnOnce(&mut Bitmap8) -> (),
    {
        self.as_ref().draw_in_rect(rect, f)
    }

    /// Draws the contents of the window on the screen as a bitmap.
    pub fn draw_into(&self, target_bitmap: &mut Bitmap8, rect: Rect) {
        self.as_ref().draw_into(target_bitmap, rect);
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
