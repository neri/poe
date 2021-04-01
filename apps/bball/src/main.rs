// bball
#![no_main]
#![no_std]

use megstd::drawing::*;
use myoslib::{window::*, *};

#[no_mangle]
fn _start() {
    let window = Window::new("bball", Size::new(200, 200));
    window.fill_rect(Rect::new(0, 0, 200, 200), WindowColor::BLACK);
    for (i, t1) in TABLE[..14].iter().enumerate() {
        for (j, t2) in TABLE[i..].iter().enumerate() {
            let dis = if j < 8 { j } else { 15 - j };
            window.draw_line(
                Point::new(t1.0, t1.1),
                Point::new(t2.0, t2.1),
                IndexedColor(16 - dis as u8),
            );
        }
    }
    window.refresh();
    window.wait_char();
}

const ADJUST_X: isize = 8;
const ADJUST_Y: isize = 29;
const TABLE: [(isize, isize); 16] = [
    (204 - ADJUST_X, 129 - ADJUST_Y),
    (195 - ADJUST_X, 90 - ADJUST_Y),
    (172 - ADJUST_X, 58 - ADJUST_Y),
    (137 - ADJUST_X, 38 - ADJUST_Y),
    (98 - ADJUST_X, 34 - ADJUST_Y),
    (61 - ADJUST_X, 46 - ADJUST_Y),
    (31 - ADJUST_X, 73 - ADJUST_Y),
    (15 - ADJUST_X, 110 - ADJUST_Y),
    (15 - ADJUST_X, 148 - ADJUST_Y),
    (31 - ADJUST_X, 185 - ADJUST_Y),
    (61 - ADJUST_X, 212 - ADJUST_Y),
    (98 - ADJUST_X, 224 - ADJUST_Y),
    (137 - ADJUST_X, 220 - ADJUST_Y),
    (172 - ADJUST_X, 200 - ADJUST_Y),
    (195 - ADJUST_X, 168 - ADJUST_Y),
    (204 - ADJUST_X, 129 - ADJUST_Y),
];
