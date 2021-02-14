// TOE Kernel
// Copyright (c) 2021 MEG-OS project

#![no_std]
#![no_main]
#![feature(asm)]

use core::fmt::Write;
use core::time::Duration;
use kernel::arch::cpu::Cpu;
use kernel::fonts::FontManager;
use kernel::graphics::bitmap::*;
use kernel::graphics::color::*;
use kernel::graphics::coords::*;
use kernel::mem::mm::MemoryManager;
use kernel::system::System;
use kernel::task::scheduler::Timer;
use kernel::window::*;
use kernel::*;
use mem::string::*;
use task::scheduler::{Scheduler, SpawnOption};
use window::WindowBuilder;
// use kernel::audio::AudioManager;
// use kernel::util::rng::XorShift32;

entry!(Application::main);

#[used]
static mut MAIN: Application = Application::new();

struct Application {}

impl Application {
    const fn new() -> Self {
        Self {}
    }

    fn main() {
        WindowManager::set_desktop_color(IndexedColor::from_rgb(0x2196F3));
        WindowManager::set_pointer_visible(true);
        Timer::sleep(Duration::from_millis(1000));

        SpawnOption::new().spawn_f(Self::status_bar_thread, 0, "status");

        let bg_color = IndexedColor::WHITE;
        let fg_color = IndexedColor::BLACK;

        let window_rect = Rect::new(10, 30, 160, 32);
        let window = WindowBuilder::new("Command Mode")
            .style_add(WindowStyle::NAKED)
            .frame(window_rect)
            .bg_color(bg_color)
            .build();
        window.make_active();

        let interval = 500;
        window.create_timer(0, Duration::from_millis(0));
        let mut sb = Sb255::new();
        let mut cursor_phase = 0;
        while let Some(message) = window.wait_message() {
            match message {
                WindowMessage::Timer(_timer) => {
                    cursor_phase ^= 1;
                    window.set_needs_display();
                    window.create_timer(0, Duration::from_millis(interval));
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
                            let padding = EdgeInsets::new(4, 4, 4, 4);
                            let font = FontManager::fixed_system_font();
                            let rect = bitmap.bounds().insets_by(padding);
                            bitmap.fill_rect(rect, bg_color);
                            font.write_str(sb.as_str(), bitmap, rect, fg_color);
                            if cursor_phase == 1 {
                                bitmap.fill_rect(
                                    Rect::new(
                                        padding.left + font.width() * sb.len() as isize,
                                        padding.top,
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
    fn about_thread(_: usize) {
        let window_size = Size::new(300, 160);
        let window = WindowBuilder::new("About").size(window_size).build();
        window.show();

        let mut sb = StringBuffer::new();
        let interval = 5000;
        window.create_timer(0, Duration::from_millis(0));
        while let Some(message) = window.wait_message() {
            match message {
                WindowMessage::Timer(_timer) => {
                    window.set_needs_display();
                    window.create_timer(0, Duration::from_millis(interval));
                }
                WindowMessage::Draw => {
                    sb.clear();

                    writeln!(sb, "{} v{}", System::name(), System::version(),).unwrap();
                    writeln!(sb, "Platform {}", System::platform(),).unwrap();
                    writeln!(sb, "CPU ver {}", System::cpu_ver().0,).unwrap();
                    writeln!(
                        sb,
                        "Memory {} KB Free, {} MB Total",
                        MemoryManager::free_memory_size() >> 10,
                        MemoryManager::total_memory_size() >> 20
                    )
                    .unwrap();

                    window
                        .draw(|bitmap| {
                            bitmap.fill_rect(bitmap.bounds(), window.bg_color());
                            let font = FontManager::fixed_system_font();
                            let mut rect = bitmap.bounds().insets_by(EdgeInsets::new(2, 4, 2, 4));
                            font.write_str(
                                sb.as_str(),
                                bitmap,
                                rect,
                                IndexedColor::from_rgb(0xCCCCCC),
                            );
                            rect.origin += Point::new(-1, -1);
                            font.write_str(sb.as_str(), bitmap, rect, IndexedColor::BLACK);
                        })
                        .unwrap();
                }
                _ => window.handle_default_message(message),
            }
        }
        unimplemented!()
    }

    #[allow(dead_code)]
    fn status_bar_thread(_: usize) {
        const STATUS_BAR_HEIGHT: isize = 24;
        let screen_size = System::main_screen().size();
        let window_size = Size::new(screen_size.width(), STATUS_BAR_HEIGHT);
        let window_rect = Rect::new(0, 0, screen_size.width(), STATUS_BAR_HEIGHT);
        let window = WindowBuilder::new("Status")
            .style(WindowStyle::BORDER | WindowStyle::FLOATING)
            .frame(window_rect)
            .build();
        window
            .draw_in_rect(window_size.into(), |bitmap| {
                let font = FontManager::fixed_system_font();
                let s = System::short_name();
                font.write_str(
                    s,
                    bitmap,
                    Rect::new(8, 2, font.width() * s.len() as isize, font.line_height()),
                    IndexedColor::BLACK,
                );
            })
            .unwrap();
        window.show();
        WindowManager::add_screen_insets(EdgeInsets::new(STATUS_BAR_HEIGHT, 0, 0, 0));

        SpawnOption::new().spawn_f(Self::about_thread, 0, "About");

        let mut sb = StringBuffer::new();

        window.create_timer(0, Duration::from_millis(0));
        while let Some(message) = window.wait_message() {
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
                    let clock_width = font.width() * 10;
                    let clock_rect = Rect::new(
                        window_size.width() - clock_width - 8,
                        (window_size.height() - font.line_height()) / 2,
                        clock_width,
                        font.line_height(),
                    );
                    window
                        .draw_in_rect(clock_rect, |bitmap| {
                            bitmap.fill_rect(bitmap.bounds(), window.bg_color());
                            font.write_str(
                                sb.as_str(),
                                bitmap,
                                bitmap.bounds(),
                                IndexedColor::BLACK,
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
