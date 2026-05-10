//! Pre-OS Execution Environment
#![no_std]
#![no_main]

extern crate alloc;

#[allow(unused_imports)]
use core::time::Duration;
use minios::io::tui::{self, prelude::box_drawing::AsciiExt};
#[allow(unused_imports)]
use minios::mem::MemoryManager;
use minios::prelude::*;

#[allow(unused_imports)]
use tui::prelude::*;

#[allow(unused_imports)]
use minios::io::graphics::PixelFormat;

pub use minios::prelude;

#[allow(unused)]
static SYSTEM_NAME: &str = "myosExp";

#[allow(unused)]
static CURRENT_VERSION: Version = Version::new(0, 0, 0, "");

pub fn main() {
    let mut exit_flag = false;
    loop {
        let stdout = System::stdout();
        stdout.reset();
        stdout.enable_cursor(false);

        let scr_size = Size::new(
            stdout.current_mode().columns as i32,
            stdout.current_mode().rows as i32,
        );

        let mut title_bar = TuiWindowBufferAscii::new(
            Rect::new(Point::new(0, 0), Size::new(scr_size.width, 1)),
            Inset::default(),
            TuiAttribute(0xf0),
        );
        title_bar.put_string_at(Point::new(1, 0), SYSTEM_NAME, title_bar.default_attr);
        title_bar.draw_to(stdout);

        #[allow(unused_mut)]
        let mut status_bar = TuiWindowBufferAscii::new(
            Rect::new(
                Point::new(0, scr_size.height - 1),
                Size::new(scr_size.width, 1),
            ),
            Inset::default(),
            TuiAttribute(0xf0),
        );
        // status_bar.put_string_at(Point::new(1, 0), "", status_bar.default_attr);
        status_bar.draw_to(stdout);

        let menu_items = [
            MainMenuItem::Start,
            MainMenuItem::TextMode,
            MainMenuItem::GraphicsMode,
        ];

        let mut menu_window = TuiWindowBufferAscii::new(
            Rect::new(Point::new(2, 2), Size::new(20, menu_items.len() as i32 + 4)),
            Inset::new(2, 2, 2, 2),
            TuiAttribute(0xf0),
        );
        menu_window.draw_box(menu_window.bounds(), menu_window.default_attr);
        menu_window.draw_simple_title(" Option ", None, menu_window.default_attr);

        menu_window.draw_to(stdout);

        let stdin = System::stdin();
        let mut needs_redraw = true;
        let mut selected_item = 0;
        loop {
            if needs_redraw {
                for (i, item) in menu_items.iter().enumerate() {
                    let attr = if selected_item == i {
                        menu_window.default_attr.reversed()
                    } else {
                        menu_window.default_attr
                    };
                    menu_window.put_string_at(Point::new(2, 2 + i as i32), item.as_str(), attr);
                }
                menu_window.redraw_if_needed(stdout);
                needs_redraw = false;
            }

            stdin.event_for_key().wait();
            if let Some(key) = stdin.read_key_stroke() {
                let usage = key.get().key_stroke().usage;
                match usage {
                    Usage::KEY_UP_ARROW => {
                        if selected_item > 0 {
                            selected_item -= 1;
                            needs_redraw = true;
                        }
                    }
                    Usage::KEY_DOWN_ARROW => {
                        if selected_item < menu_items.len() - 1 {
                            selected_item += 1;
                            needs_redraw = true;
                        }
                    }
                    Usage::KEY_ENTER => {
                        let selected = menu_items[selected_item];
                        match selected {
                            MainMenuItem::TextMode => {
                                System::conctl().set_text_mode();
                                break;
                            }
                            MainMenuItem::GraphicsMode => {
                                stdout.reset();
                                let _ = System::conctl().set_graphics_mode_from_list(&[
                                    // (1920, 1080, PixelFormat::BGRX8888),
                                    // (1280, 720, PixelFormat::BGRX8888),
                                    // (800, 600, PixelFormat::BGRX8888),
                                    // (800, 600, PixelFormat::Indexed8),
                                    (640, 480, PixelFormat::Indexed8),
                                    (320, 200, PixelFormat::Indexed8),
                                ]);
                                break;
                            }
                            MainMenuItem::Start => {
                                exit_flag = true;
                                break;
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        if exit_flag {
            break;
        }
    }
    #[allow(unreachable_code)]
    {}

    if true {
        let stdout = System::stdout();
        stdout.reset();
        stdout.enable_cursor(false);

        let scr_size = Size::new(
            stdout.current_mode().columns as i32,
            stdout.current_mode().rows as i32,
        );

        let window_size = Size::new(20, 7);
        let window_pos = Point::new(
            (scr_size.width - window_size.width) / 2,
            (scr_size.height - window_size.height) / 2,
        );
        let mut window = TuiWindowBufferAscii::new(
            Rect::new(window_pos, window_size),
            Inset::new(2, 2, 2, 2),
            TuiAttribute(0xf0),
        );

        window.draw_box(window.bounds(), window.default_attr);
        window.draw_simple_title(" Start ", None, TuiAttribute(0xf0));
        window.put_string_at(Point::new(2, 2), "Starting up...", window.default_attr);
        window.draw_hline(
            Point::new(2, 4),
            window_size.width - 4,
            AsciiExt::from_char(' ').unwrap(),
            TuiAttribute(0x70),
        );

        window.draw_to(stdout);

        for i in 0..(window_size.width - 4) {
            window.put_char_at(
                Point::new(i + 2, 4),
                AsciiExt::from_char(' ').unwrap(),
                TuiAttribute(0x10),
            );
            window.redraw_if_needed(stdout);

            let mut timer = Event::with_timeout(Duration::from_millis(100));
            timer.wait();
        }

        stdout.set_attribute(0xb7);
        stdout.clear_screen();
    }

    if true {
        let stdout = System::stdout();
        // stdout.reset();
        // stdout.enable_cursor(false);
        // stdout.set_attribute(0xb7);
        // stdout.clear_screen();

        {
            let scr_size = (
                stdout.current_mode().columns as i32,
                stdout.current_mode().rows as i32,
            );

            let mut title_bar = TuiWindowBufferAscii::new(
                Rect::new(Point::new(0, 0), Size::new(scr_size.0, 1)),
                Inset::default(),
                TuiAttribute(0xf0),
            );
            title_bar.put_string_at(Point::new(1, 0), SYSTEM_NAME, title_bar.default_attr);
            title_bar.draw_to(stdout);

            let mut status_bar = TuiWindowBufferAscii::new(
                Rect::new(Point::new(0, scr_size.1 - 1), Size::new(scr_size.0, 1)),
                Inset::default(),
                TuiAttribute(0xf0),
            );
            status_bar.put_string_at(Point::new(1, 0), " Status: None ", status_bar.default_attr);
            status_bar.draw_to(stdout);

            let mut window = TuiWindowBufferAscii::new(
                Rect::new(Point::new(2, 2), Size::new(20, 10)),
                Inset::new(2, 2, 2, 2),
                TuiAttribute(0xf0),
            );

            window.draw_box(window.bounds(), TuiAttribute(0xf0));
            // window.draw_simple_title("Hello", TuiAttribute(0x9f).into(), TuiAttribute(0x0f));
            window.draw_simple_title(" Hello ", None, TuiAttribute(0xf0));
            window.put_string_at(Point::new(2, 2), "Hello, world!", window.default_attr);
            window.put_text(Point::new(2, 4), "Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat. Duis aute irure dolor in reprehenderit in voluptate velit esse cillum dolore eu fugiat nulla pariatur. Excepteur sint occaecat cupidatat non proident, sunt in culpa qui officia deserunt mollit anim id est laborum.", TuiAttribute(0x06), 0);

            window.draw_to(stdout);
        }

        stdout.set_attribute(0xb0);
        println!("");
        println!("");
    }

    // loop {
    //     // let duration = Duration::from_millis(500);
    //     let duration = Duration::from_millis(500);

    //     let mut timer = Event::with_timeout(duration);
    //     timer.wait();

    //     // print!(".");
    //     println!(
    //         "Monotonic: {}, {}, {}",
    //         Platform::monotonic(),
    //         duration.subsec_nanos(),
    //         Platform::duration_to_ticks(duration),
    //     );
    // }

    // Hal::cpu().bad_instruction();
    #[allow(unreachable_code)]
    {}

    //-//-//-//-//-//-//-//-//-//-//-//-//-//-//-//-//-//-//-//-//-//-

    // let info = System::boot_info();
    // let memsize1 = MemoryManager::total_memory_size();
    // let memsize2 = MemoryManager::total_extended_memory_size();
    // println!("{} v{}", SYSTEM_NAME, CURRENT_VERSION,);
    // if memsize2 > 0 {
    //     let memsize1 = (memsize1 + 0xfffff) >> 20;
    //     let memsize = memsize1 + memsize2;
    //     print!(
    //         "MEMORY {} GB ({} MB + {} MB)",
    //         (memsize + 0x3ff) >> 10,
    //         memsize1,
    //         memsize2,
    //     );
    // } else {
    //     let memsize1 = (memsize1 + 0x3ff) >> 10;
    //     print!("MEMORY {} MB ({} KB)", (memsize1 + 0x3ff) >> 10, memsize1,);
    // }
    // println!(", PLATFORM {}", info.platform);
    // println!("");

    loop {
        print!("poe>");
        if let Some(line) = line_input(64) {
            if line.is_empty() {
                continue;
            }
            println!("{:?}: Bad command or file name.", line);
        }
    }
}

pub fn line_input(max_len: usize) -> Option<String> {
    let mut buf = Vec::with_capacity(max_len);
    let stdin = System::stdin();
    let stdout = System::stdout();

    loop {
        stdout.enable_cursor(true);
        stdin.event_for_key().wait();
        match stdin.read_key_stroke() {
            Some(key) => {
                stdout.enable_cursor(false);
                let key = key.get();
                let c = key.unicode_char().unwrap_or_default();
                match c {
                    '\x00' => {
                        // non-character key
                        write!(
                            stdout,
                            "[#{:02x}{:02x}]",
                            key.key_stroke().modifier.bits(),
                            key.key_stroke().usage.0,
                        )
                        .unwrap();
                    }
                    // ctrl-c
                    '\x03' => {
                        write!(stdout, "^C\r\n").unwrap();
                        return None;
                    }
                    // backspace
                    '\x08' | '\x7f' => match buf.pop() {
                        Some(c) => {
                            if c < ' ' {
                                write!(stdout, "\x08\x08  \x08\x08").unwrap();
                            } else {
                                write!(stdout, "\x08 \x08").unwrap();
                            }
                        }
                        None => {}
                    },
                    // enter
                    '\x0a' | '\x0d' => {
                        write!(stdout, "\r\n").unwrap();
                        break;
                    }
                    _ => {
                        if buf.len() < max_len {
                            if c < ' ' {
                                // control char
                                stdout.write_char('^').unwrap();
                                stdout.write_char((c as u8 | 0x40) as char).unwrap();
                                buf.push(c);
                            } else if c <= '\x7E' {
                                // printable char
                                let _ = stdout.write_char(c);
                                buf.push(c);
                            } else {
                                // TODO: unprintable char
                            }
                        }
                    }
                }
            }
            None => {}
        }
    }
    Some(buf.into_iter().collect())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum MainMenuItem {
    Start,
    TextMode,
    GraphicsMode,
}

impl MainMenuItem {
    pub fn as_str(&self) -> &'static str {
        match self {
            MainMenuItem::Start => "Start",
            MainMenuItem::TextMode => "Text Mode",
            MainMenuItem::GraphicsMode => "Graphics Mode",
        }
    }
}
