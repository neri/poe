// TOE Kernel
// Copyright (c) 2021 MEG-OS project

#![no_std]
#![no_main]
#![feature(asm)]

use arch::cpu::Cpu;
use audio::AudioManager;
use core::{fmt::Write, time::Duration};
use kernel::fonts::FontManager;
use kernel::graphics::bitmap::*;
use kernel::graphics::color::*;
use kernel::graphics::coords::*;
use kernel::system::System;
use kernel::*;
use task::scheduler::Timer;
use util::rng::XorShift32;
use window::WindowManager;

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
        println!("Platform: {}", System::platform(),);

        {
            let screen = bitmap;
            let mut rng = XorShift32::default();
            let bitmap =
                OsBitmap8::from_bytes(&BITMAP_DATA, Size::new(BITMAP_WIDTH, BITMAP_HEIGHT));
            for _ in 0..100 {
                let x = (rng.next() % 240) as isize;
                let y = (rng.next() % 200) as isize;
                screen.blt(
                    &bitmap,
                    Point::new(x, y),
                    bitmap.bounds(),
                    BltOption::empty(),
                );
            }
        }

        loop {
            unsafe {
                Cpu::halt();
            }
            if let Some(key) = WindowManager::get_key() {
                print!("{}", key);
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
