// TOE Kernel
// Copyright (c) 2021 MEG-OS project

#![no_std]
#![no_main]
#![feature(asm)]

use core::{alloc::Layout, fmt::Write, time::Duration};
use kernel::arch::cpu::Cpu;
use kernel::audio::AudioManager;
use kernel::fonts::FontManager;
use kernel::graphics::bitmap::*;
use kernel::graphics::color::*;
use kernel::graphics::coords::*;
use kernel::mem::mm::MemoryManager;
use kernel::system::System;
use kernel::task::scheduler::Timer;
use kernel::util::rng::XorShift32;
use kernel::window::WindowManager;
use kernel::*;

entry!(Application::main);

#[used]
static mut MAIN: Application = Application::new();

struct Application {}

impl Application {
    const fn new() -> Self {
        Self {}
    }

    fn main() {
        let bitmap = System::main_screen();
        let size = bitmap.size();

        bitmap.fill_rect(Rect::from(size), IndexedColor::BLUE);
        // bitmap.fill_rect(Rect::from(size), IndexedColor::LIGHT_CYAN);
        // bitmap.fill_rect(Rect::new(0, 0, size.width(), 24), IndexedColor::LIGHT_GRAY);
        // bitmap.draw_hline(Point::new(0, 22), size.width(), IndexedColor::DARK_GRAY);
        // bitmap.draw_hline(Point::new(0, 23), size.width(), IndexedColor::BLACK);

        let font = FontManager::fixed_system_font();

        if false {
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

        if false {
            let window_rect = Rect::new(240, 200, 160, 100);
            let coords = unsafe { Coordinates::from_rect_unchecked(window_rect) };
            bitmap.fill_round_rect(window_rect, 1, IndexedColor::LIGHT_GRAY);

            bitmap.draw_hline(
                coords.left_top() + Point::new(1, 1),
                window_rect.width() - 3,
                IndexedColor::WHITE,
            );
            bitmap.draw_vline(
                coords.left_top() + Point::new(1, 1),
                window_rect.height() - 3,
                IndexedColor::WHITE,
            );
            bitmap.draw_vline(
                coords.right_top() + Point::new(-2, 2),
                window_rect.height() - 3,
                IndexedColor::DARK_GRAY,
            );
            bitmap.draw_hline(
                coords.left_bottom() + Point::new(2, -2),
                window_rect.width() - 3,
                IndexedColor::DARK_GRAY,
            );
            bitmap.draw_round_rect(window_rect, 1, IndexedColor::BLACK);
        }

        println!("{} v{}", System::name(), System::version(),);
        println!("Platform {}", System::platform(),);
        println!(
            "Memory {} KB Free, {} MB Total",
            MemoryManager::free_memory_size() >> 10,
            MemoryManager::total_memory_size() >> 20
        );

        if false {
            let screen = bitmap;
            let mut rng = XorShift32::default();
            let bitmap =
                OsBitmap8::from_bytes(&BITMAP_DATA, Size::new(BITMAP_WIDTH, BITMAP_HEIGHT));
            for _ in 0..100 {
                let x = (rng.next() % 300) as isize;
                let y = (rng.next() % 180) as isize;
                screen.blt_with_key(
                    &bitmap,
                    Point::new(x, y),
                    bitmap.bounds(),
                    IndexedColor(0xFF),
                );
            }
        }

        print!("# ");
        loop {
            if let Some(key) = WindowManager::get_key() {
                match key {
                    'r' => unsafe { Cpu::reset() },
                    '\x08' => print!(" \x08\x08"),
                    '\r' => print!(" \n# "),
                    _ => print!("{}", key),
                }
            } else {
                print!("\x7F\x08");
                unsafe {
                    Cpu::halt();
                }
            }
        }
    }
}

const BITMAP_WIDTH: isize = 16;
const BITMAP_HEIGHT: isize = 16;
static BITMAP_DATA: [u8; (BITMAP_WIDTH * BITMAP_HEIGHT) as usize] = [
    0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
    0xFF, 0xFF, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0xFF, 0xFF,
    0xFF, 0xFF, 0x02, 0x0A, 0x0A, 0x0A, 0x0A, 0x0A, 0x0A, 0x0A, 0x0A, 0x0A, 0x0A, 0x02, 0xFF, 0xFF,
    0xFF, 0xFF, 0x02, 0x0A, 0x00, 0x00, 0x0A, 0x0A, 0x0A, 0x0A, 0x00, 0x00, 0x0A, 0x02, 0xFF, 0xFF,
    0xFF, 0xFF, 0x02, 0x0A, 0x00, 0x00, 0x0A, 0x0A, 0x0A, 0x0A, 0x00, 0x00, 0x0A, 0x02, 0xFF, 0xFF,
    0xFF, 0xFF, 0x02, 0x0A, 0x0A, 0x0A, 0x0A, 0x00, 0x00, 0x0A, 0x0A, 0x0A, 0x0A, 0x02, 0xFF, 0xFF,
    0xFF, 0xFF, 0x02, 0x0A, 0x0A, 0x0A, 0x00, 0x00, 0x00, 0x00, 0x0A, 0x0A, 0x0A, 0x02, 0xFF, 0xFF,
    0xFF, 0xFF, 0x02, 0x0A, 0x0A, 0x0A, 0x00, 0x0A, 0x0A, 0x00, 0x0A, 0x0A, 0x0A, 0x02, 0xFF, 0xFF,
    0xFF, 0xFF, 0x02, 0x0A, 0x0A, 0x0A, 0x0A, 0x0A, 0x0A, 0x0A, 0x0A, 0x0A, 0x0A, 0x02, 0xFF, 0xFF,
    0xFF, 0xFF, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0xFF, 0xFF,
    0xFF, 0xFF, 0xFF, 0x0F, 0x00, 0x0F, 0x00, 0x0F, 0x00, 0x0F, 0x0F, 0x00, 0x0F, 0xFF, 0xFF, 0xFF,
    0xFF, 0xFF, 0xFF, 0x0F, 0x00, 0x0F, 0x00, 0x0F, 0x00, 0x0F, 0x0F, 0x00, 0x0F, 0xFF, 0xFF, 0xFF,
    0xFF, 0x0F, 0x0F, 0x00, 0x0F, 0x0F, 0x00, 0x0F, 0x00, 0x0F, 0x0F, 0x00, 0x0F, 0xFF, 0x0F, 0xFF,
    0x0F, 0x00, 0x0F, 0x00, 0x0F, 0x00, 0x0F, 0x0F, 0x00, 0x0F, 0x0F, 0x00, 0x0F, 0x0F, 0x00, 0x0F,
    0xFF, 0x0F, 0x00, 0x0F, 0x0F, 0x00, 0x0F, 0x0F, 0x00, 0x0F, 0xFF, 0x0F, 0x00, 0x00, 0x0F, 0xFF,
    0xFF, 0xFF, 0x0F, 0xFF, 0xFF, 0x0F, 0xFF, 0xFF, 0x0F, 0xFF, 0xFF, 0xFF, 0x0F, 0x0F, 0xFF, 0xFF,
];
