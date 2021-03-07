// TOE Kernel
// Copyright (c) 2021 MEG-OS project

#![no_std]
#![no_main]
#![feature(asm)]

use crate::task::*;
use alloc::vec::Vec;
use core::fmt::Write;
use core::time::Duration;
use kernel::arch::cpu::Cpu;
use kernel::fonts::FontManager;
use kernel::graphics::bitmap::*;
use kernel::graphics::color::*;
use kernel::graphics::coords::*;
use kernel::mem::MemoryManager;
use kernel::system::System;
use kernel::task::scheduler::*;
use kernel::util::text::*;
use kernel::window::*;
use kernel::*;
use mem::string::*;
use window::WindowBuilder;
// use kernel::audio::AudioManager;
// use kernel::util::rng::XorShift32;

extern crate alloc;

entry!(Shell::main);

#[used]
static mut MAIN: Shell = Shell::new();

struct Shell {}

impl Shell {
    const fn new() -> Self {
        Self {}
    }

    fn main() {
        WindowManager::set_desktop_color(TrueColor::from_rgb(0x2196F3).into());
        WindowManager::set_pointer_visible(true);
        // Timer::sleep(Duration::from_millis(100));

        // SpawnOption::new().spawn_f(Self::test_thread, 0, "Test");

        Scheduler::spawn_async(Task::new(Self::status_bar_main()));
        Scheduler::spawn_async(Task::new(Self::activity_monitor_main()));
        Scheduler::spawn_async(Task::new(Self::console_main()));
        // Scheduler::spawn_async(Task::new(Self::about_main()));
        Scheduler::perform_tasks();
    }

    #[allow(dead_code)]
    fn test_thread(_: usize) {
        loop {
            Cpu::noop();
        }
    }

    async fn console_main() {
        let padding_x = 4;
        let padding_y = 4;
        let font = FontManager::fixed_system_font();
        let bg_color = AmbiguousColor::from(IndexedColor::WHITE);
        let fg_color = AmbiguousColor::from(IndexedColor::BLACK);

        let window_rect = Rect::new(8, 30, 128, font.line_height() + padding_y * 2);
        let window = WindowBuilder::new("Terminal")
            .style_add(WindowStyle::NAKED)
            .frame(window_rect)
            .bg_color(bg_color.into())
            .build();
        window.make_active();

        let interval = 500;
        window.create_timer(0, Duration::from_millis(0));
        let mut sb = Sb255::new();
        let mut cursor_phase = 0;
        while let Some(message) = window.get_message().await {
            match message {
                WindowMessage::Activated | WindowMessage::Deactivated => {
                    window.set_needs_display();
                }
                WindowMessage::Timer(_timer) => {
                    cursor_phase ^= 1;
                    window.create_timer(0, Duration::from_millis(interval));
                    if window.is_active() {
                        window.set_needs_display();
                    }
                }
                WindowMessage::Char(c) => {
                    match c {
                        '\x08' => sb.backspace(),
                        '\x0D' => sb.clear(),
                        _ => {
                            let _ = sb.write_char(c);
                        }
                    }
                    window.set_needs_display();
                }
                WindowMessage::Draw => {
                    window
                        .draw(|bitmap| {
                            let rect = Rect::new(
                                padding_x,
                                padding_y,
                                bitmap.size().width() as isize - padding_x * 2,
                                font.line_height(),
                            );
                            bitmap.fill_rect(rect, bg_color.into());
                            TextProcessing::write_str(
                                bitmap,
                                sb.as_str(),
                                font,
                                Point::new(rect.x(), rect.y()),
                                fg_color.into(),
                            );
                            if window.is_active() && cursor_phase == 1 {
                                bitmap.fill_rect(
                                    Rect::new(
                                        rect.x() + font.width() * sb.len() as isize,
                                        rect.y(),
                                        font.width(),
                                        font.line_height(),
                                    ),
                                    fg_color,
                                );
                            }
                        })
                        .unwrap();
                }
                _ => window.handle_default_message(message),
            }
        }
        unimplemented!()
    }

    #[allow(dead_code)]
    async fn activity_monitor_main() {
        let window_size = Size::new(280, 160);
        let bg_color = AmbiguousColor::from(IndexedColor::BLACK);
        let fg_color = AmbiguousColor::from(IndexedColor::YELLOW);
        let graph_sub_color = AmbiguousColor::from(IndexedColor::LIGHT_GREEN);
        let graph_main_color = AmbiguousColor::from(IndexedColor::YELLOW);
        let graph_border_color = AmbiguousColor::from(IndexedColor::LIGHT_GRAY);

        let window = WindowBuilder::new("Activity Monitor")
            .style_add(WindowStyle::PINCHABLE)
            // .style_add(WindowStyle::FLOATING)
            .frame(Rect::new(
                -window_size.width - 8,
                -window_size.height - 8,
                window_size.width,
                window_size.height,
            ))
            .bg_color(bg_color)
            .build();
        window.show();

        let n_items = 64;
        let mut usage_cursor = 0;
        let mut usage_history = {
            let mut vec = Vec::with_capacity(n_items);
            vec.resize(n_items, u8::MAX);
            vec
        };

        let mut sb = StringBuffer::with_capacity(0x1000);

        let interval = 1000;
        window.create_timer(0, Duration::from_millis(0));
        while let Some(message) = window.get_message().await {
            match message {
                WindowMessage::Timer(_timer) => {
                    window.set_needs_display();
                    window.create_timer(0, Duration::from_millis(interval));
                }
                WindowMessage::Draw => {
                    let font = FontManager::fixed_small_font();
                    sb.clear();

                    let max_value = 1000;
                    let usage = Scheduler::usage_total();
                    usage_history[usage_cursor] =
                        (254 * usize::min(max_value, max_value - usage) / max_value) as u8;
                    usage_cursor = (usage_cursor + 1) % n_items;

                    writeln!(
                        sb,
                        "Memory {} MB, {} KB Free, {} KB Used",
                        MemoryManager::total_memory_size() >> 20,
                        MemoryManager::free_memory_size() >> 10,
                        (MemoryManager::total_memory_size()
                            - MemoryManager::free_memory_size()
                            - 0x100000)
                            >> 10,
                    )
                    .unwrap();
                    Scheduler::print_statistics(&mut sb, true);

                    window
                        .draw(|bitmap| {
                            bitmap.fill_rect(bitmap.bounds(), window.bg_color().into());

                            {
                                let padding = 4;
                                let item_size = Size::new(n_items as isize, 32);
                                let rect =
                                    Rect::new(padding, padding, item_size.width, item_size.height);
                                // let cursor = rect.x() + rect.width() + padding;

                                let h_lines = 4;
                                let v_lines = 4;
                                for i in 1..h_lines {
                                    let point = Point::new(
                                        rect.x(),
                                        rect.y() + i * item_size.height / h_lines,
                                    );
                                    bitmap.draw_hline(point, item_size.width, graph_sub_color);
                                }
                                for i in 1..v_lines {
                                    let point = Point::new(
                                        rect.x() + i * item_size.width / v_lines,
                                        rect.y(),
                                    );
                                    bitmap.draw_vline(point, item_size.height, graph_sub_color);
                                }

                                let limit = item_size.width as usize - 2;
                                for i in 0..limit {
                                    let scale = item_size.height - 2;
                                    let value1 = usage_history
                                        [((usage_cursor + i - limit) % n_items)]
                                        as isize
                                        * scale
                                        / 255;
                                    let value2 = usage_history
                                        [((usage_cursor + i - 1 - limit) % n_items)]
                                        as isize
                                        * scale
                                        / 255;
                                    let c0 = Point::new(
                                        rect.x() + i as isize + 1,
                                        rect.y() + 1 + value1,
                                    );
                                    let c1 =
                                        Point::new(rect.x() + i as isize, rect.y() + 1 + value2);
                                    bitmap.draw_line(c0, c1, graph_main_color);
                                }
                                bitmap.draw_rect(rect, graph_border_color);
                            }

                            let rect = bitmap.bounds().insets_by(EdgeInsets::new(40, 4, 4, 4));
                            TextProcessing::draw_text(
                                bitmap,
                                sb.as_str(),
                                font,
                                rect,
                                fg_color.into(),
                                0,
                                LineBreakMode::default(),
                                TextAlignment::Left,
                                util::text::VerticalAlignment::Top,
                            );
                        })
                        .unwrap();
                }
                _ => window.handle_default_message(message),
            }
        }
        unimplemented!()
    }

    #[allow(dead_code)]
    async fn about_main() {
        let window_size = Size::new(320, 160);
        let window = WindowBuilder::new("About").size(window_size).build();
        window.show();

        let mut sb = StringBuffer::new();
        let interval = 5000;
        window.create_timer(0, Duration::from_millis(0));
        while let Some(message) = window.get_message().await {
            match message {
                WindowMessage::Timer(_timer) => {
                    window.set_needs_display();
                    window.create_timer(0, Duration::from_millis(interval));
                }
                WindowMessage::Draw => {
                    let font = FontManager::fixed_ui_font();
                    sb.clear();

                    writeln!(sb, "{} v{}", System::name(), System::version(),).unwrap();
                    writeln!(sb, "Platform {}", System::platform(),).unwrap();
                    writeln!(sb, "CPU ver {}", System::cpu_ver().0,).unwrap();
                    writeln!(sb, "Memory {} MB", MemoryManager::total_memory_size() >> 20,)
                        .unwrap();

                    window
                        .draw(|bitmap| {
                            bitmap.fill_rect(bitmap.bounds(), window.bg_color());
                            let rect = bitmap.bounds().insets_by(EdgeInsets::new(0, 8, 2, 8));
                            TextProcessing::draw_text(
                                bitmap,
                                sb.as_str(),
                                font,
                                rect,
                                IndexedColor::BLACK.into(),
                                0,
                                LineBreakMode::default(),
                                TextAlignment::Center,
                                util::text::VerticalAlignment::Bottom,
                            );
                        })
                        .unwrap();
                }
                _ => window.handle_default_message(message),
            }
        }
        unimplemented!()
    }

    #[allow(dead_code)]
    async fn status_bar_main() {
        const STATUS_BAR_HEIGHT: isize = 24;
        let screen_size = System::main_screen().size();
        let window_size = Size::new(screen_size.width(), STATUS_BAR_HEIGHT);
        let window_rect = Rect::new(0, 0, screen_size.width(), STATUS_BAR_HEIGHT);
        let window = WindowBuilder::new("Status")
            .style(WindowStyle::BORDER | WindowStyle::FLOATING)
            .frame(window_rect)
            // .bg_color(IndexedColor::from_rgb(0xCCCCFF))
            .build();
        window
            .draw_in_rect(window_size.into(), |bitmap| {
                let font = FontManager::fixed_ui_font();
                let s = System::short_name();
                {
                    TextProcessing::write_str(
                        bitmap,
                        s,
                        font,
                        Point::new(9, (STATUS_BAR_HEIGHT - font.line_height()) / 2),
                        IndexedColor::BLACK.into(),
                    );
                }
            })
            .unwrap();
        window.show();
        WindowManager::add_screen_insets(EdgeInsets::new(STATUS_BAR_HEIGHT, 0, 0, 0));

        let mut sb = StringBuffer::new();

        window.create_timer(0, Duration::from_millis(0));
        while let Some(message) = window.get_message().await {
            match message {
                WindowMessage::Timer(_timer) => {
                    let time = System::system_time();
                    let interval = 1_000_000_000 - time.nanos as u64;
                    window.create_timer(0, Duration::from_nanos(interval));
                    window.set_needs_display();
                }
                WindowMessage::Draw => {
                    sb.clear();
                    let time = System::system_time();
                    let tod = time.secs % 86400;
                    let min = tod / 60 % 60;
                    let hour = tod / 3600;
                    if true {
                        let sec = tod % 60;
                        if sec % 2 == 0 {
                            write!(sb, "{:2} {:02} {:02}", hour, min, sec).unwrap();
                        } else {
                            write!(sb, "{:2}:{:02}:{:02}", hour, min, sec).unwrap();
                        };
                    } else {
                        write!(sb, "{:2}:{:02}", hour, min).unwrap();
                    }

                    let font = FontManager::fixed_system_font();
                    let clock_width = font.width() * 8;
                    let clock_rect = Rect::new(
                        window_size.width() - clock_width - 8,
                        (window_size.height() - font.line_height()) / 2,
                        clock_width,
                        font.line_height(),
                    );
                    window
                        .draw_in_rect(clock_rect, |bitmap| {
                            bitmap.fill_rect(bitmap.bounds(), window.bg_color());
                            TextProcessing::write_str(
                                bitmap,
                                sb.as_str(),
                                font,
                                Point::default(),
                                IndexedColor::BLACK.into(),
                            );
                        })
                        .unwrap();
                    window.invalidate_rect(clock_rect);
                }
                _ => window.handle_default_message(message),
            }
        }
        unimplemented!()
    }
}

// const BITMAP_WIDTH: isize = 16;
// const BITMAP_HEIGHT: isize = 16;
// static BITMAP_DATA: [u8; (BITMAP_WIDTH * BITMAP_HEIGHT) as usize] = [
//     0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
//     0xFF, 0xFF, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0xFF, 0xFF,
//     0xFF, 0xFF, 0x02, 0x0A, 0x0A, 0x0A, 0x0A, 0x0A, 0x0A, 0x0A, 0x0A, 0x0A, 0x0A, 0x02, 0xFF, 0xFF,
//     0xFF, 0xFF, 0x02, 0x0A, 0x00, 0x00, 0x0A, 0x0A, 0x0A, 0x0A, 0x00, 0x00, 0x0A, 0x02, 0xFF, 0xFF,
//     0xFF, 0xFF, 0x02, 0x0A, 0x00, 0x00, 0x0A, 0x0A, 0x0A, 0x0A, 0x00, 0x00, 0x0A, 0x02, 0xFF, 0xFF,
//     0xFF, 0xFF, 0x02, 0x0A, 0x0A, 0x0A, 0x0A, 0x00, 0x00, 0x0A, 0x0A, 0x0A, 0x0A, 0x02, 0xFF, 0xFF,
//     0xFF, 0xFF, 0x02, 0x0A, 0x0A, 0x0A, 0x00, 0x00, 0x00, 0x00, 0x0A, 0x0A, 0x0A, 0x02, 0xFF, 0xFF,
//     0xFF, 0xFF, 0x02, 0x0A, 0x0A, 0x0A, 0x00, 0x0A, 0x0A, 0x00, 0x0A, 0x0A, 0x0A, 0x02, 0xFF, 0xFF,
//     0xFF, 0xFF, 0x02, 0x0A, 0x0A, 0x0A, 0x0A, 0x0A, 0x0A, 0x0A, 0x0A, 0x0A, 0x0A, 0x02, 0xFF, 0xFF,
//     0xFF, 0xFF, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0xFF, 0xFF,
//     0xFF, 0xFF, 0xFF, 0x0F, 0x00, 0x0F, 0x00, 0x0F, 0x00, 0x0F, 0x0F, 0x00, 0x0F, 0xFF, 0xFF, 0xFF,
//     0xFF, 0xFF, 0xFF, 0x0F, 0x00, 0x0F, 0x00, 0x0F, 0x00, 0x0F, 0x0F, 0x00, 0x0F, 0xFF, 0xFF, 0xFF,
//     0xFF, 0x0F, 0x0F, 0x00, 0x0F, 0x0F, 0x00, 0x0F, 0x00, 0x0F, 0x0F, 0x00, 0x0F, 0xFF, 0x0F, 0xFF,
//     0x0F, 0x00, 0x0F, 0x00, 0x0F, 0x00, 0x0F, 0x0F, 0x00, 0x0F, 0x0F, 0x00, 0x0F, 0x0F, 0x00, 0x0F,
//     0xFF, 0x0F, 0x00, 0x0F, 0x0F, 0x00, 0x0F, 0x0F, 0x00, 0x0F, 0xFF, 0x0F, 0x00, 0x00, 0x0F, 0xFF,
//     0xFF, 0xFF, 0x0F, 0xFF, 0xFF, 0x0F, 0xFF, 0xFF, 0x0F, 0xFF, 0xFF, 0xFF, 0x0F, 0x0F, 0xFF, 0xFF,
// ];
