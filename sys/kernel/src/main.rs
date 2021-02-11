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

        SpawnOption::new().spawn_f(Self::status_bar_thread, 0, "status");

        Timer::sleep(Duration::from_millis(1000));

        SpawnOption::new().spawn(Self::key_thread, 0, "key");
        for i in 1..5 {
            SpawnOption::new().spawn(Self::thread_test, i, "test");
        }
        // Timer::sleep(Duration::from_millis(500));

        {
            let window_size = Size::new(240, 150);
            let window = WindowBuilder::new("Hello").size(window_size).build();
            window
                .draw_in_rect(window_size.into(), |bitmap| {
                    let font = FontManager::fixed_system_font();
                    font.write_str("It works!", bitmap, Point::new(10, 10), IndexedColor::BLACK);
                })
                .unwrap();
            window.make_active();
        }

        println!("\n\n");
        println!("{} v{}", System::name(), System::version(),);
        println!("Platform {}", System::platform(),);
        println!("CPU ver {}", System::cpu_ver().0,);
        println!(
            "Memory {} KB Free, {} MB Total",
            MemoryManager::free_memory_size() >> 10,
            MemoryManager::total_memory_size() >> 20
        );

        loop {
            Timer::sleep(Duration::from_millis(1000));
        }
    }

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
                font.write_str(
                    System::short_name(),
                    bitmap,
                    Point::new(8, 2),
                    IndexedColor::BLACK,
                );
            })
            .unwrap();
        window.show();
        WindowManager::add_screen_insets(EdgeInsets::new(STATUS_BAR_HEIGHT, 0, 0, 0));

        let mut sb = StringBuffer::new();

        let interval = 500;
        window.create_timer(0, Duration::from_millis(interval));
        loop {
            if let Some(message) = window.wait_message() {
                match message {
                    WindowMessage::Timer(_timer) => {
                        window.set_needs_display();
                        window.create_timer(0, Duration::from_millis(interval));
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
                                bitmap.fill_rect(bitmap.bounds(), IndexedColor::WHITE);
                                font.write_str(
                                    sb.as_str(),
                                    bitmap,
                                    Point::default(),
                                    IndexedColor::BLACK,
                                );
                            })
                            .unwrap();
                        window.invalidate_rect(clock_rect);
                    }
                    _ => window.handle_default_message(message),
                }
            }
        }
    }

    fn key_thread(_: usize) {
        let window = WindowBuilder::new("Key Test")
            .frame(Rect::new(-160, 40, 150, 50))
            .build();
        window.show();

        let mut sb = Sb255::new();
        loop {
            if let Some(message) = window.wait_message() {
                match message {
                    WindowMessage::Draw => {
                        window
                            .draw(|bitmap| {
                                bitmap.fill_rect(bitmap.bounds(), IndexedColor::WHITE);
                                let font = FontManager::fixed_system_font();
                                font.write_str(
                                    sb.as_str(),
                                    bitmap,
                                    Point::new(10, 0),
                                    IndexedColor::BLACK,
                                );
                            })
                            .unwrap();
                    }
                    WindowMessage::Char(c) => {
                        match c {
                            '\x08' => sb.backspace(),
                            _ => sb.write_char(c).unwrap(),
                        }
                        window.set_needs_display();
                    }
                    _ => window.handle_default_message(message),
                }
            }
        }
    }

    fn thread_test(i: usize) {
        let window = WindowBuilder::new("Thread Test")
            .frame(Rect::new(-160, 40 + i as isize * 60, 150, 50))
            .build();
        window.show();

        let mut sb = StringBuffer::new();
        let mut counter = 0;
        let interval = 100;
        window.create_timer(0, Duration::from_millis(interval));
        loop {
            if let Some(message) = window.wait_message() {
                match message {
                    WindowMessage::Timer(_timer) => {
                        window.set_needs_display();
                        window.create_timer(0, Duration::from_millis(interval));
                    }
                    WindowMessage::Draw => {
                        sformat!(sb, "{}", counter);

                        window
                            .draw(|bitmap| {
                                bitmap.fill_rect(bitmap.bounds(), IndexedColor::WHITE);
                                let font = FontManager::fixed_system_font();
                                font.write_str(
                                    sb.as_str(),
                                    bitmap,
                                    Point::new(10, 0),
                                    IndexedColor::BLACK,
                                );
                            })
                            .unwrap();

                        counter += 1;
                    }
                    _ => window.handle_default_message(message),
                }
            }
        }
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
