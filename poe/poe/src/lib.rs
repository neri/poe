//! Pre-OS Execution Environment
#![no_std]
#![no_main]

extern crate alloc;
use minios::io::tui;
#[allow(unused_imports)]
use minios::mem::MemoryManager;
use minios::prelude::*;

#[allow(unused_imports)]
use minios::io::graphics::PixelFormat;

pub use minios::prelude;

#[allow(unused)]
static SYSTEM_NAME: &str = "POE";

#[allow(unused)]
static CURRENT_VERSION: Version = Version::new(0, 0, 0, "");

pub fn main() {
    if true {
        let _ = System::conctl().set_graphics_mode_from_list(&[
            // (1920, 1080, PixelFormat::BGRX8888),
            (1280, 720, PixelFormat::BGRX8888),
            // (800, 600, PixelFormat::BGRX8888),
            // (800, 600, PixelFormat::Indexed8),
            (640, 480, PixelFormat::Indexed8),
            (320, 200, PixelFormat::Indexed8),
        ]);

        let stdout = System::stdout();
        stdout.reset();
        stdout.enable_cursor(false);
        stdout.set_attribute(0xb7);
        stdout.clear_screen();

        {
            use tui::prelude::*;

            let mut window = TuiWindowBufferAscii::new(
                Rect::new(Point::new(2, 2), Size::new(20, 10)),
                Inset::new(2, 2, 2, 2),
                TuiAttribute(0xf0),
            );

            window.draw_box(window.bounds(), TuiAttribute(0xf0));
            // window.draw_simple_title("Hello", TuiAttribute(0x9f).into(), TuiAttribute(0x0f));
            window.draw_simple_title(" Hello ", None, TuiAttribute(0xf0));
            window.put_string_at(Point::new(2, 2), "Hello, world!", window.default_attr);
            window.put_text(Point::new(2, 4), "Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat. Duis aute irure dolor in reprehenderit in voluptate velit esse cillum dolore eu fugiat nulla pariatur. Excepteur sint occaecat cupidatat non proident, sunt in culpa qui officia deserunt mollit anim id est laborum.", TuiAttribute(0x07), 0);

            window.draw_to(stdout);
        }

        stdout.set_attribute(0xb0);
        println!("");
        println!("");
    }

    // Hal::cpu().bad_instruction();
    // #[allow(unreachable_code)]
    // {}

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

    // println!("* SUPER POE SHELL v0.0 *");
    loop {
        print!(">");
        if let Some(line) = System::line_input(64) {
            if line.is_empty() {
                continue;
            }
            println!("{:?}: Bad command or file name.", line);
        }
    }
}
