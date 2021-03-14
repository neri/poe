// A Window System

use crate::arch::cpu::Cpu;
use crate::drawing::*;
use crate::fonts::*;
use crate::sync::atomicflags::AtomicBitflags;
use crate::sync::fifo::*;
use crate::sync::semaphore::*;
use crate::task::scheduler::*;
use crate::task::AtomicWaker;
use crate::util::text::*;
use crate::*;
use crate::{io::hid::*, system::System};
use alloc::boxed::Box;
use alloc::collections::btree_map::BTreeMap;
use alloc::sync::Arc;
use alloc::vec::Vec;
use bitflags::*;
use core::cell::UnsafeCell;
use core::cmp;
use core::future::Future;
use core::num::NonZeroUsize;
use core::pin::Pin;
use core::sync::atomic::*;
use core::task::{Context, Poll};
use core::time::Duration;

// use core::fmt::Write;

static mut WM: Option<Box<WindowManager<'_>>> = None;

const MAX_WINDOWS: usize = 255;
const WINDOW_TITLE_LENGTH: usize = 32;

const WINDOW_BORDER_PADDING: isize = 1;
const WINDOW_TITLE_HEIGHT: isize = 20;

const WINDOW_DEFAULT_KEY_COLOR: IndexedColor = IndexedColor::DEFAULT_KEY;
const WINDOW_BORDER_COLOR: AmbiguousColor = AmbiguousColor::from_rgb(0x666666);
const WINDOW_DEFAULT_BGCOLOR: AmbiguousColor = AmbiguousColor::from_rgb(0xFFFFFF);
const WINDOW_ACTIVE_TITLE_BG_COLOR: AmbiguousColor = AmbiguousColor::from_rgb(0xCCCCCC);
const WINDOW_ACTIVE_TITLE_FG_COLOR: AmbiguousColor = AmbiguousColor::from_rgb(0x333333);
const WINDOW_INACTIVE_TITLE_BG_COLOR: AmbiguousColor = AmbiguousColor::from_rgb(0xFFFFFF);
const WINDOW_INACTIVE_TITLE_FG_COLOR: AmbiguousColor = AmbiguousColor::from_rgb(0x999999);

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

pub struct WindowManager<'a> {
    main_screen: Bitmap<'static>,
    screen_insets: EdgeInsets,

    window_pool: BTreeMap<WindowHandle, Arc<UnsafeCell<Box<RawWindow<'a>>>>>,
    window_orders: Vec<WindowHandle>,
    sem_winthread: Semaphore,
    attributes: AtomicBitflags<WindowManagerAttributes>,

    pointer_x: AtomicIsize,
    pointer_y: AtomicIsize,
    buttons: AtomicUsize,
    buttons_down: AtomicUsize,
    buttons_up: AtomicUsize,

    root: WindowHandle,
    pointer: WindowHandle,

    active: Option<WindowHandle>,
    captured: Option<WindowHandle>,
    captured_origin: Point,
    entered: Option<WindowHandle>,
}

bitflags! {
    struct WindowManagerAttributes: usize {
        const MOUSE_MOVE    = 0b0000_0001;
        const NEEDS_REDRAW  = 0b0000_0010;
        const MOVING        = 0b0000_0100;
    }
}

impl Into<usize> for WindowManagerAttributes {
    fn into(self) -> usize {
        self.bits()
    }
}

impl WindowManager<'static> {
    pub(crate) unsafe fn init() {
        let main_screen = System::main_screen();
        let pointer_x = AtomicIsize::new(main_screen.width() as isize / 2);
        let pointer_y = AtomicIsize::new(main_screen.height() as isize / 2);

        let mut window_pool = BTreeMap::new();
        let mut window_orders = Vec::with_capacity(MAX_WINDOWS);

        let root = {
            let window = WindowBuilder::new("Root")
                .style(WindowStyle::NAKED)
                .level(WindowLevel::ROOT)
                .frame(main_screen.bounds())
                .bg_color(IndexedColor::BLACK.into())
                .without_bitmap()
                .without_message_queue()
                .build_inner();

            let handle = window.handle;
            window_pool.insert(handle, Arc::new(UnsafeCell::new(window)));
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
                .bitmap_strategy(BitmapStrategy::Compact)
                .without_message_queue()
                .build_inner();

            window
                .draw_in_rect(pointer_size.into(), |bitmap| {
                    let cursor = ConstBitmap8::from_bytes(&MOUSE_POINTER_SOURCE, pointer_size);
                    bitmap.blt(&cursor, Point::new(0, 0), pointer_size.into())
                })
                .unwrap();

            let handle = window.handle;
            window_pool.insert(handle, Arc::new(UnsafeCell::new(window)));
            handle
        };

        WM = Some(Box::new(Self {
            screen_insets: EdgeInsets::default(),
            main_screen,
            window_pool,
            window_orders,
            sem_winthread: Semaphore::new(0),
            attributes: AtomicBitflags::EMPTY,
            pointer_x,
            pointer_y,
            buttons: AtomicUsize::new(0),
            buttons_down: AtomicUsize::new(0),
            buttons_up: AtomicUsize::new(0),
            active: None,
            captured: None,
            captured_origin: Point::default(),
            entered: None,
            root,
            pointer,
        }));

        SpawnOption::with_priority(Priority::High).spawn(Self::window_thread, 0, "Window Manager");
    }

    #[inline]
    pub fn main_screen<'a>(&self) -> &'a mut Bitmap<'static> {
        &mut WindowManager::shared_mut().main_screen
    }

    fn add(window: Box<RawWindow<'static>>) {
        unsafe {
            Cpu::without_interrupts(|| {
                let shared = WindowManager::shared_mut();
                let handle = window.handle;
                shared
                    .window_pool
                    .insert(handle, Arc::new(UnsafeCell::new(window)));
            })
        }
    }

    #[allow(dead_code)]
    fn remove(_window: &WindowHandle) {
        // TODO:
    }
}

impl WindowManager<'static> {
    #[inline]
    fn get<'a>(&self, key: &WindowHandle) -> Option<&'a Box<RawWindow<'static>>> {
        unsafe {
            Cpu::without_interrupts(|| self.window_pool.get(key))
                .map(|v| v.clone().get())
                .map(|v| &(*v))
        }
    }

    fn get_mut<F, R>(&mut self, key: &WindowHandle, f: F) -> Option<R>
    where
        F: FnOnce(&mut RawWindow) -> R,
    {
        let window = unsafe {
            Cpu::without_interrupts(move || self.window_pool.get_mut(key).map(|v| v.clone()))
        };
        window.map(|window| unsafe {
            let window = window.get();
            f(&mut *window)
        })
    }
}

impl WindowManager<'_> {
    #[inline]
    #[track_caller]
    fn shared<'a>() -> &'a WindowManager<'static> {
        unsafe { WM.as_ref().unwrap() }
    }

    #[inline]
    #[track_caller]
    fn shared_mut<'a>() -> &'a mut WindowManager<'static> {
        unsafe { WM.as_mut().unwrap() }
    }

    #[inline]
    fn shared_opt<'a>() -> Option<&'a Box<WindowManager<'static>>> {
        unsafe { WM.as_ref() }
    }

    fn window_thread(_: usize) {
        let shared = unsafe { WM.as_mut().unwrap() };
        loop {
            shared.sem_winthread.wait();

            if shared
                .attributes
                .test_and_clear(WindowManagerAttributes::NEEDS_REDRAW)
            {
                let desktop = shared.root;
                desktop.as_ref().draw_to_screen(desktop.frame());
            }
            if shared
                .attributes
                .test_and_clear(WindowManagerAttributes::MOUSE_MOVE)
            {
                let position = shared.pointer();
                let current_buttons =
                    MouseButton::from_bits_truncate(shared.buttons.load(Ordering::Acquire) as u8);
                let buttons_down = MouseButton::from_bits_truncate(
                    shared.buttons_down.swap(0, Ordering::SeqCst) as u8,
                );
                let buttons_up = MouseButton::from_bits_truncate(
                    shared.buttons_up.swap(0, Ordering::SeqCst) as u8,
                );

                if let Some(captured) = shared.captured {
                    if current_buttons.contains(MouseButton::LEFT) {
                        if shared.attributes.contains(WindowManagerAttributes::MOVING) {
                            let top = if captured.as_ref().level < WindowLevel::FLOATING {
                                shared.screen_insets.top
                            } else {
                                0
                            };
                            let x = position.x - shared.captured_origin.x;
                            let y = cmp::max(position.y - shared.captured_origin.y, top);
                            captured.move_to(Point::new(x, y));
                        } else {
                            let _ = Self::make_mouse_events(
                                captured,
                                position,
                                current_buttons,
                                buttons_down,
                                buttons_up,
                            );
                        }
                    } else {
                        let _ = Self::make_mouse_events(
                            captured,
                            position,
                            current_buttons,
                            buttons_down,
                            buttons_up,
                        );
                        shared.captured = None;
                        shared.attributes.remove(WindowManagerAttributes::MOVING);

                        let target = Self::window_at_point(position);
                        if let Some(entered) = shared.entered {
                            if entered != target {
                                let _ = Self::make_mouse_events(
                                    captured,
                                    position,
                                    current_buttons,
                                    MouseButton::empty(),
                                    MouseButton::empty(),
                                );
                                let _ = entered.post(WindowMessage::MouseLeave);
                                shared.entered = Some(target);
                                let _ = target.post(WindowMessage::MouseEnter);
                            }
                        }
                    }
                } else {
                    let target = Self::window_at_point(position);

                    if buttons_down.contains(MouseButton::LEFT) {
                        if let Some(active) = shared.active {
                            if active != target {
                                WindowManager::set_active(Some(target));
                            }
                        } else {
                            WindowManager::set_active(Some(target));
                        }
                        let target_window = target.as_ref();
                        if target_window.style.contains(WindowStyle::PINCHABLE) {
                            shared.attributes.insert(WindowManagerAttributes::MOVING);
                        } else {
                            let mut title_frame = target_window.title_frame();
                            title_frame.origin += target_window.frame.origin;
                            if position.is_within(title_frame) {
                                shared.attributes.insert(WindowManagerAttributes::MOVING);
                            } else {
                                let _ = Self::make_mouse_events(
                                    target,
                                    position,
                                    current_buttons,
                                    buttons_down,
                                    buttons_up,
                                );
                            }
                        }
                        shared.captured = Some(target);
                        shared.captured_origin = position - target_window.frame.origin;
                    } else {
                        let _ = Self::make_mouse_events(
                            target,
                            position,
                            current_buttons,
                            buttons_down,
                            buttons_up,
                        );
                    }

                    if let Some(entered) = shared.entered {
                        if entered != target {
                            let _ = entered.post(WindowMessage::MouseLeave);
                            shared.entered = Some(target);
                            let _ = target.post(WindowMessage::MouseEnter);
                        }
                    }
                }

                shared.pointer.move_to(position);
            }
        }
    }

    fn make_mouse_events(
        target: WindowHandle,
        position: Point,
        buttons: MouseButton,
        down: MouseButton,
        up: MouseButton,
    ) -> Result<(), WindowPostError> {
        let window = target.as_ref();
        let origin = window.frame.insets_by(window.content_insets).origin;
        let point = Point::new(position.x - origin.x, position.y - origin.y);

        if down.is_empty() && up.is_empty() {
            return target.post(WindowMessage::MouseMove(MouseEvent::new(
                point,
                buttons,
                MouseButton::empty(),
            )));
        }
        let mut errors = None;
        if !down.is_empty() {
            match target.post(WindowMessage::MouseDown(MouseEvent::new(
                point, buttons, down,
            ))) {
                Ok(_) => (),
                Err(err) => errors = Some(err),
            };
        }
        if !up.is_empty() {
            match target.post(WindowMessage::MouseUp(MouseEvent::new(point, buttons, up))) {
                Ok(_) => (),
                Err(err) => errors = Some(err),
            };
        }
        match errors {
            Some(err) => Err(err),
            None => Ok(()),
        }
    }

    #[inline]
    pub fn is_enabled() -> bool {
        unsafe { WM.is_some() }
    }

    /// SAFETY: MUST lock window_orders
    unsafe fn add_hierarchy(window: WindowHandle) {
        let window = match window.get() {
            Some(v) => v,
            None => return,
        };

        let shared = WindowManager::shared_mut();
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

        let shared = WindowManager::shared_mut();
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
        match WindowManager::shared_opt() {
            Some(shared) => shared.main_screen.bounds(),
            None => System::main_screen().size().into(),
        }
    }

    #[inline]
    pub fn user_screen_bounds() -> Rect {
        match WindowManager::shared_opt() {
            Some(shared) => shared.main_screen.bounds().insets_by(shared.screen_insets),
            None => System::main_screen().size().into(),
        }
    }

    #[inline]
    pub fn screen_insets() -> EdgeInsets {
        let shared = WindowManager::shared();
        shared.screen_insets
    }

    #[inline]
    pub fn add_screen_insets(insets: EdgeInsets) {
        let shared = WindowManager::shared_mut();
        shared.screen_insets += insets;
    }

    pub(crate) fn post_key_event(event: KeyEvent) {
        let shared = match WindowManager::shared_opt() {
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
        } else if let Some(window) = shared.active {
            let _ = window.post(WindowMessage::Key(event));
        }
    }

    pub fn post_mouse_event(mouse_state: &mut MouseState) {
        let shared = match WindowManager::shared_opt() {
            Some(v) => v,
            None => return,
        };

        let screen_bounds = Self::main_screen_bounds();

        let mut pointer = Point::new(0, 0);
        core::mem::swap(&mut mouse_state.x, &mut pointer.x);
        core::mem::swap(&mut mouse_state.y, &mut pointer.y);
        let button_changes: MouseButton = mouse_state.current_buttons ^ mouse_state.prev_buttons;
        let button_down: MouseButton = button_changes & mouse_state.current_buttons;
        let button_up: MouseButton = button_changes & mouse_state.prev_buttons;
        let button_changed = !button_changes.is_empty();

        if button_changed {
            shared.buttons.store(
                mouse_state.current_buttons.bits() as usize,
                Ordering::SeqCst,
            );
            shared
                .buttons_down
                .fetch_or(button_down.bits() as usize, Ordering::SeqCst);
            shared
                .buttons_up
                .fetch_or(button_up.bits() as usize, Ordering::SeqCst);
        }

        let moved = Cpu::interlocked_add_clamp(
            &shared.pointer_x,
            pointer.x,
            screen_bounds.x(),
            screen_bounds.width() - 1,
        ) | Cpu::interlocked_add_clamp(
            &shared.pointer_y,
            pointer.y,
            screen_bounds.y(),
            screen_bounds.height() - 1,
        );

        if button_changed | moved {
            shared
                .attributes
                .insert(WindowManagerAttributes::MOUSE_MOVE);
            shared.sem_winthread.signal();
        }
    }

    #[inline]
    pub fn invalidate_screen(rect: Rect) {
        let shared = WindowManager::shared();
        let _ = shared.root.update_opt(|root| {
            root.invalidate_rect(rect);
        });
    }

    pub fn set_desktop_color(color: AmbiguousColor) {
        let shared = WindowManager::shared();
        let _ = shared.root.update_opt(|root| {
            root.bitmap = None;
            root.set_bg_color(color);
        });
    }

    // pub fn set_desktop_bitmap(bitmap: Option<BoxedBitmap>) {
    //     let shared = Self::shared();
    //     let _ = shared.root.update_opt(|root| {
    //         root.bitmap = bitmap.map(|v| UnsafeCell::new(v));
    //         root.set_needs_display();
    //     });
    // }

    fn window_at_point(point: Point) -> WindowHandle {
        unsafe {
            Cpu::without_interrupts(|| {
                let shared = WindowManager::shared();
                for handle in shared.window_orders.iter().rev().skip(1) {
                    let window = handle.as_ref();
                    if point.is_within(window.frame) {
                        return *handle;
                    }
                }
                shared.root
            })
        }
    }

    #[inline]
    fn pointer(&self) -> Point {
        Point::new(
            self.pointer_x.load(Ordering::Relaxed),
            self.pointer_y.load(Ordering::Relaxed),
        )
    }

    #[inline]
    pub fn is_pointer_visible() -> bool {
        WindowManager::shared()
            .pointer
            .get()
            .map(|v| v.is_visible())
            .unwrap_or(false)
    }

    pub fn set_pointer_visible(visible: bool) -> bool {
        WindowManager::shared()
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

    pub fn save_screen_to(bitmap: &mut Bitmap, rect: Rect) {
        let shared = WindowManager::shared();
        Self::while_hiding_pointer(|| shared.root.update(|v| v.draw_into(bitmap, rect)));
    }

    fn set_active(window: Option<WindowHandle>) {
        let shared = WindowManager::shared_mut();
        if let Some(old_active) = shared.active {
            let _ = old_active.post(WindowMessage::Deactivated);
            shared.active = window;
            let _ = old_active.update_opt(|window| window.refresh_title());
        } else {
            shared.active = window;
        }
        if let Some(active) = window {
            let _ = active.post(WindowMessage::Activated);
            active.show();
        }
    }
}

/// Raw implementation of the window
#[allow(dead_code)]
struct RawWindow<'a> {
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
    bg_color: AmbiguousColor,
    key_color: IndexedColor,
    bitmap: Option<UnsafeCell<BoxedBitmap<'a>>>,

    /// Window Title
    title: [u8; WINDOW_TITLE_LENGTH],

    // Messages and Events
    waker: AtomicWaker,
    sem: Semaphore,
    queue: Option<InterlockedFifo<WindowMessage>>,
}

bitflags! {
    pub struct WindowStyle: u8 {
        const BORDER        = 0b0000_0001;
        const TITLE         = 0b0000_0010;
        const NAKED         = 0b0000_0100;
        const TRANSPARENT        = 0b0000_1000;
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

impl RawWindow<'_> {
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
        WindowManager::invalidate_screen(self.frame);
        // self.set_needs_display();
    }

    fn hide(&self) {
        let shared = WindowManager::shared_mut();
        let frame = self.frame;
        let handle = self.handle;
        let new_active = if shared.active.contains(&handle) {
            unsafe {
                Cpu::without_interrupts(|| {
                    shared
                        .window_orders
                        .iter()
                        .position(|v| *v == handle)
                        .and_then(|v| shared.window_orders.get(v - 1))
                        .map(|&v| v)
                })
            }
        } else {
            None
        };
        if shared.captured.contains(&self.handle) {
            shared.captured = None;
        }
        unsafe {
            Cpu::without_interrupts(|| {
                WindowManager::remove_hierarchy(self.handle);
            });
        }
        WindowManager::invalidate_screen(frame);
        WindowManager::set_active(new_active);
    }

    fn set_frame(&mut self, new_frame: Rect) {
        let old_frame = self.frame;
        if old_frame != new_frame {
            self.frame = new_frame;
            if self.attributes.contains(WindowAttributes::VISIBLE) {
                self.draw_frame();

                let coords1 = match Coordinates::from_rect(old_frame) {
                    Ok(v) => v,
                    Err(_) => return,
                };
                let coords2 = match Coordinates::from_rect(new_frame) {
                    Ok(v) => v,
                    Err(_) => return,
                };
                let new_coords = Coordinates::new(
                    isize::min(coords1.left, coords2.left),
                    isize::min(coords1.top, coords2.top),
                    isize::max(coords1.right, coords2.right),
                    isize::max(coords1.bottom, coords2.bottom),
                );
                WindowManager::invalidate_screen(new_coords.into());
            }
        }
    }

    fn set_bg_color(&mut self, color: AmbiguousColor) {
        self.bg_color = color;
        if let Some(mut bitmap) = self.bitmap() {
            bitmap.fill_rect(bitmap.bounds(), color.into());
            self.draw_frame();
        }
        self.set_needs_display();
    }

    fn title_frame(&self) -> Rect {
        if self.style.contains(WindowStyle::TITLE) {
            Rect::new(
                WINDOW_BORDER_PADDING,
                WINDOW_BORDER_PADDING,
                self.frame.width() - WINDOW_BORDER_PADDING * 2,
                WINDOW_TITLE_HEIGHT - WINDOW_BORDER_PADDING,
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
                    Point::new(WINDOW_BORDER_PADDING, WINDOW_TITLE_HEIGHT),
                    title_rect.width(),
                    WINDOW_BORDER_COLOR,
                );

                if let Some(s) = self.title() {
                    let rect = title_rect.insets_by(EdgeInsets::new(0, 8, 0, 8));
                    AttributedString::props()
                        .font(FontManager::title_font())
                        .color(if is_active {
                            WINDOW_ACTIVE_TITLE_FG_COLOR
                        } else {
                            WINDOW_INACTIVE_TITLE_FG_COLOR
                        })
                        .center()
                        .text(s)
                        .draw_text(&mut bitmap, rect, 1);
                }
            }
        }
    }

    fn draw_to_screen(&self, rect: Rect) {
        let mut frame = rect;
        frame.origin += self.frame.origin;
        let main_screen = WindowManager::shared().main_screen();
        self.draw_into(main_screen, frame);
    }

    fn draw_into(&self, target_bitmap: &mut Bitmap, frame: Rect) -> bool {
        let coords1 = match Coordinates::from_rect(frame) {
            Ok(v) => v,
            Err(_) => return false,
        };

        let shared = WindowManager::shared();

        let first_index = if self.style.contains(WindowStyle::TRANSPARENT) {
            0
        } else {
            shared
                .window_orders
                .iter()
                .position(|&v| v == self.handle)
                .unwrap_or(0)
        };

        for handle in &shared.window_orders[first_index..] {
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

                    if let Some(bitmap) = window.bitmap() {
                        if window.style.contains(WindowStyle::TRANSPARENT) {
                            target_bitmap.blt_transparent(
                                &bitmap,
                                blt_origin,
                                blt_rect,
                                window.key_color,
                            );
                        } else {
                            target_bitmap.blt(&bitmap, blt_origin, blt_rect);
                        }
                    } else {
                        target_bitmap.fill_rect(blt_rect, window.bg_color.into());
                    }
                }
            })
        }

        true
    }

    #[inline]
    pub fn set_needs_display(&self) {
        match self.handle.post(WindowMessage::Draw) {
            Ok(()) => (),
            Err(_) => {
                let shared = WindowManager::shared();
                shared
                    .attributes
                    .insert(WindowManagerAttributes::NEEDS_REDRAW);
                shared.sem_winthread.signal();
            }
        }
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
            self.invalidate_rect(self.title_frame());
        }
    }
}

impl<'a> RawWindow<'a> {
    #[inline]
    fn bitmap(&self) -> Option<Bitmap<'a>> {
        self.bitmap
            .as_ref()
            .and_then(|v| unsafe { v.get().as_mut() })
            .map(|v| v.as_bitmap())
    }

    fn title<'b>(&self) -> Option<&'b str> {
        let len = self.title[0] as usize;
        match len {
            0 => None,
            _ => core::str::from_utf8(unsafe { core::slice::from_raw_parts(&self.title[1], len) })
                .ok(),
        }
    }

    fn draw_in_rect<F>(&self, rect: Rect, f: F) -> Result<(), WindowDrawingError>
    where
        F: FnOnce(&mut Bitmap) -> (),
    {
        let mut bitmap = match self.bitmap() {
            Some(bitmap) => bitmap,
            None => return Err(WindowDrawingError::NoBitmap),
        };
        let bounds = Rect::from(self.frame.size).insets_by(self.content_insets);
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
        match bitmap.view(rect, |mut bitmap| f(&mut bitmap)) {
            Some(_) => Ok(()),
            None => Err(WindowDrawingError::InconsistentCoordinates),
        }
    }
}

pub struct WindowBuilder {
    frame: Rect,
    style: WindowStyle,
    level: WindowLevel,
    bg_color: AmbiguousColor,
    key_color: IndexedColor,
    title: [u8; WINDOW_TITLE_LENGTH],
    queue_size: usize,
    bitmap_strategy: BitmapStrategy,
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
            queue_size: 32,
            bitmap_strategy: BitmapStrategy::default(),
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

    fn build_inner<'a>(mut self) -> Box<RawWindow<'a>> {
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

        let queue = match self.queue_size {
            0 => None,
            _ => Some(InterlockedFifo::new(self.queue_size)),
        };

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
            queue,
            sem: Semaphore::new(0),
            waker: AtomicWaker::new(),
        });

        match self.bitmap_strategy {
            BitmapStrategy::NonBitmap => (),
            BitmapStrategy::Native => {
                window.bitmap = Some(UnsafeCell::new(BoxedBitmap::same_format(
                    WindowManager::shared().main_screen(),
                    frame.size(),
                    self.bg_color,
                )));
            }
            BitmapStrategy::Compact => {
                window.bitmap = Some(UnsafeCell::new(
                    BoxedBitmap8::new(frame.size(), self.bg_color.into()).into(),
                ));
            }
            BitmapStrategy::Expressive => {
                window.bitmap = Some(UnsafeCell::new(
                    BoxedBitmap32::new(frame.size(), self.bg_color.into()).into(),
                ));
            }
        }

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
    pub const fn position(mut self, position: Point) -> Self {
        self.frame.origin = position;
        self
    }

    #[inline]
    pub const fn size(mut self, size: Size) -> Self {
        self.frame.size = size;
        self
    }

    #[inline]
    pub const fn bg_color(mut self, bg_color: AmbiguousColor) -> Self {
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
        self.bitmap_strategy = BitmapStrategy::NonBitmap;
        self
    }

    #[inline]
    pub const fn bitmap_strategy(mut self, bitmap_strategy: BitmapStrategy) -> Self {
        self.bitmap_strategy = bitmap_strategy;
        self
    }
}

pub enum BitmapStrategy {
    NonBitmap,
    Native,
    Compact,
    Expressive,
}

impl Default for BitmapStrategy {
    fn default() -> Self {
        Self::Compact
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
        static NEXT_ID: AtomicUsize = AtomicUsize::new(1);
        Self::new(Cpu::interlocked_increment(&NEXT_ID)).unwrap()
    }

    #[inline]
    fn get<'a>(&self) -> Option<&'a Box<RawWindow<'static>>> {
        WindowManager::shared().get(self)
    }

    #[inline]
    #[track_caller]
    fn as_ref<'a>(&self) -> &'a RawWindow<'static> {
        self.get().unwrap()
    }

    #[inline]
    fn update_opt<F, R>(&self, f: F) -> Option<R>
    where
        F: FnOnce(&mut RawWindow) -> R,
    {
        WindowManager::shared_mut().get_mut(self, f)
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
    pub fn is_active(&self) -> bool {
        self.get().map(|v| v.is_active()).unwrap_or(false)
    }

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

    pub fn set_bg_color(&self, color: AmbiguousColor) {
        self.update(|window| {
            window.set_bg_color(color);
        });
    }

    #[inline]
    pub fn bg_color(&self) -> AmbiguousColor {
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
        self.as_ref().set_needs_display();
    }

    pub fn refresh_if_needed(&self) {
        let window = match self.get() {
            Some(v) => v,
            None => return,
        };
        if window
            .attributes
            .test_and_clear(WindowAttributes::NEEDS_REDRAW)
        {
            let _ = self.draw(|_| {});
        }
    }

    #[inline]
    pub fn draw<F>(&self, f: F) -> Result<(), WindowDrawingError>
    where
        F: FnOnce(&mut Bitmap) -> (),
    {
        self.update(|window| {
            let rect = window.actual_bounds().insets_by(window.content_insets);
            match self.draw_in_rect(rect.size().into(), f) {
                Ok(_) | Err(WindowDrawingError::NoBitmap) => {
                    window.invalidate_rect(rect);
                    Ok(())
                }
                Err(err) => Err(err),
            }
        })
    }

    pub fn draw_in_rect<F>(&self, rect: Rect, f: F) -> Result<(), WindowDrawingError>
    where
        F: FnOnce(&mut Bitmap) -> (),
    {
        self.as_ref().draw_in_rect(rect, f)
    }

    /// Draws the contents of the window on the screen as a bitmap.
    pub fn draw_into(&self, target_bitmap: &mut Bitmap, rect: Rect) {
        self.as_ref().draw_into(target_bitmap, rect);
    }

    /// Post a window message.
    pub fn post(&self, message: WindowMessage) -> Result<(), WindowPostError> {
        match self.update_opt(|window| {
            if let Some(queue) = window.queue.as_mut() {
                match message {
                    WindowMessage::Draw => {
                        window.attributes.insert(WindowAttributes::NEEDS_REDRAW);
                        window.waker.wake();
                        window.sem.signal();
                        Ok(())
                    }
                    _ => queue
                        .enqueue(message)
                        .map_err(|_| WindowPostError::Full)
                        .map(|_| {
                            window.waker.wake();
                            window.sem.signal();
                        }),
                }
            } else {
                Err(WindowPostError::NotFound)
            }
        }) {
            Some(v) => v,
            None => Err(WindowPostError::NotFound),
        }
    }

    /// Read a window message from the message queue.
    pub fn read_message(&self) -> Option<WindowMessage> {
        self.update_opt(|window| {
            if let Some(queue) = window.queue.as_mut() {
                match queue.dequeue() {
                    Some(v) => Some(v),
                    _ => {
                        if window
                            .attributes
                            .test_and_clear(WindowAttributes::NEEDS_REDRAW)
                        {
                            Some(WindowMessage::Draw)
                        } else {
                            None
                        }
                    }
                }
            } else {
                None
            }
        })
        .and_then(|v| v)
    }

    /// Wait for window messages to be read.
    pub fn wait_message(&self) -> Option<WindowMessage> {
        loop {
            let window = match self.get() {
                Some(window) => window,
                None => return None,
            };
            match self.read_message() {
                Some(message) => return Some(message),
                None => window.sem.wait(),
            }
        }
    }

    /// Supports asynchronous reading of window messages.
    pub fn poll_message(&self, cx: &mut Context<'_>) -> Option<WindowMessage> {
        self.as_ref().waker.register(cx.waker());
        self.read_message().map(|message| {
            self.as_ref().waker.take();
            message
        })
    }

    /// Get the window message asynchronously.
    pub fn get_message(&self) -> Pin<Box<dyn Future<Output = Option<WindowMessage>>>> {
        Box::pin(WindowMessageConsumer { handle: *self })
    }

    /// Process window messages that are not handled.
    pub fn handle_default_message(&self, message: WindowMessage) {
        match message {
            WindowMessage::Draw => {
                self.draw(|_bitmap| {}).unwrap();
            }
            WindowMessage::Key(key) => {
                if let Some(c) = key.key_data().map(|v| v.into_char()) {
                    let _ = self.post(WindowMessage::Char(c));
                }
            }
            _ => (),
        }
    }

    /// Create a timer associated with a window
    pub fn create_timer(&self, timer_id: usize, duration: Duration) {
        let mut event = TimerEvent::window(*self, timer_id, Timer::new(duration));
        loop {
            if event.until() {
                match Scheduler::schedule_timer(event) {
                    Ok(()) => break,
                    Err(e) => event = e,
                }
            } else {
                break event.fire();
            }
        }
    }
}

struct WindowMessageConsumer {
    handle: WindowHandle,
}

impl Future for WindowMessageConsumer {
    type Output = Option<WindowMessage>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.handle.poll_message(cx) {
            Some(v) => Poll::Ready(Some(v)),
            None => Poll::Pending,
        }
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

#[non_exhaustive]
#[derive(Debug, Copy, Clone)]
pub enum WindowMessage {
    /// Dummy message
    Nop,
    /// Requested to close the window
    Close,
    /// Needs to be redrawn
    Draw,
    /// Active
    Activated,
    Deactivated,
    /// Raw keyboard event
    Key(KeyEvent),
    /// Unicode converted keyboard event
    Char(char),
    /// mouse events
    MouseMove(MouseEvent),
    MouseDown(MouseEvent),
    MouseUp(MouseEvent),
    MouseEnter,
    MouseLeave,
    /// Timer event
    Timer(usize),
    /// User Defined
    User(usize),
}

impl Default for WindowMessage {
    fn default() -> Self {
        Self::Nop
    }
}
