use core::time::Duration;

use minios::io::tui::prelude::box_drawing::AsciiExt;

use crate::*;

pub fn tui_demo() {
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
    let mut window = TuiWindowBufferA::new(
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

        let mut title_bar = TuiWindowBufferA::new(
            Rect::new(Point::new(0, 0), Size::new(scr_size.0, 1)),
            Inset::default(),
            TuiAttribute(0xf0),
        );
        title_bar.put_string_at(Point::new(1, 0), SYSTEM_NAME, title_bar.default_attr);
        title_bar.draw_to(stdout);

        let mut status_bar = TuiWindowBufferA::new(
            Rect::new(Point::new(0, scr_size.1 - 1), Size::new(scr_size.0, 1)),
            Inset::default(),
            TuiAttribute(0xf0),
        );
        status_bar.put_string_at(Point::new(1, 0), " Status: None ", status_bar.default_attr);
        status_bar.draw_to(stdout);

        let mut window = TuiWindowBufferA::new(
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
