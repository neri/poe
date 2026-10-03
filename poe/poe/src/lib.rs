//! Pre-OS Execution Environment
#![no_std]
#![no_main]

extern crate alloc;

#[allow(unused_imports)]
use core::time::Duration;

#[allow(unused_imports)]
use minios::io::graphics::PixelFormat;
use minios::io::graphics::PreferredGraphicsMode;
use minios::io::tui::{self};
#[allow(unused_imports)]
use minios::mem::MemoryManager;
pub use minios::prelude;
use minios::prelude::*;
use tui::prelude::*;

#[cfg(feature = "sd")]
mod sdblk;
#[cfg(feature = "usb")]
mod usbblk;
#[cfg(feature = "virtio")]
mod virtio;

mod tui_demo;

#[allow(unused)]
static SYSTEM_NAME: &str = "myosExp";

#[allow(unused)]
static CURRENT_VERSION: Version = Version::new(0, 0, 0, "");

pub fn main() {
    System::set_graphics_mode_if_recommended();

    cmd_about();
    loop {
        print!("poe>");
        if let Some(line) = line_input(64) {
            let mut args = line.split_whitespace();
            let Some(cmd) = args.next() else {
                continue;
            };
            match cmd {
                "about" => cmd_about(),
                "memmap" => cmd_memmap(),
                "handoff" => cmd_handoff(),
                "reboot" => System::reset_system(),
                "clear" => System::stdout().clear_screen(),
                "tui" => tui_demo::tui_demo(),
                "mode" => mode(args),
                #[cfg(feature = "usb")]
                "usbstat" => cmd_usbstat(),
                #[cfg(feature = "usb")]
                "lsusb" => cmd_lsusb(args),
                #[cfg(feature = "usb")]
                "usbblk" | "usbread" | "usbwrite" | "usbbench" => usbblk::command(cmd, args),
                #[cfg(feature = "virtio")]
                "virq" | "vrng" | "vblk" | "vread" | "vwrite" | "vflush" | "vreset" => {
                    virtio::command(cmd, args)
                }
                #[cfg(feature = "sd")]
                "sdblk" | "sdread" | "sdtest" | "sdreset" => sdblk::command(cmd, args),
                _ => println!("{:?}: Bad command or file name.", cmd),
            }
        }
    }
}

/// `core::fmt::Write` onto the console, counting what went through.
#[cfg(feature = "usb")]
struct Console {
    written: usize,
}

#[cfg(feature = "usb")]
impl core::fmt::Write for Console {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        self.written += s.len();
        print!("{}", s);
        Ok(())
    }
}

/// Prints the state the USB service reports: root port, enumeration stage,
/// devices, transfer counters and the controller registers.
#[cfg(feature = "usb")]
fn cmd_usbstat() {
    let mut console = Console { written: 0 };
    let _ = System::describe_services(&mut console);
    if console.written == 0 {
        println!("No USB service reports its state.");
    }
}

/// `lsusb` lists the devices on every USB bus as a tree; `lsusb 1-2.3`
/// (or `2.3` with a single bus) decodes that device's descriptors.
#[cfg(feature = "usb")]
fn cmd_lsusb<'a>(mut args: impl Iterator<Item = &'a str>) {
    use minios::io::usb::inventory;

    let buses = System::usb_buses();
    let mut console = Console { written: 0 };
    match args.next() {
        None if buses.is_empty() => println!("No USB bus."),
        None => {
            let _ = inventory::write_tree(&mut console, &buses);
        }
        Some(name) => match inventory::find(&buses, name) {
            Some((bus, device)) => {
                let _ = inventory::write_details(&mut console, bus, device);
            }
            None => println!(
                "lsusb: no device {} (usage: lsusb [bus-port.port...])",
                name
            ),
        },
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

/// Display the physical memory map.
pub fn cmd_memmap() {
    for region in MemoryManager::regions() {
        println!("{}", region);
    }
    let (len, peak, capacity) = MemoryManager::table_usage();
    println!(
        "free {} KB (largest {} KB), table {} entries (peak {}) of {}",
        MemoryManager::free_memory_count() >> 10,
        MemoryManager::max_free_memory_size() >> 10,
        len,
        peak,
        capacity
    );
}

/// Test of the handoff to an OS: stops the devices, writes the final memory
/// map, ends MiniOS and stops where the jump to the OS would be. Never
/// returns once the map is written.
pub fn cmd_handoff() {
    if let Err(err) = System::prepare_exit() {
        println!("handoff: devices not stopped: {:?}", err);
        return;
    }
    let map = match unsafe { MemoryManager::finalize_map() } {
        Ok(map) => map,
        Err(err) => {
            println!("handoff: no final map: {:?}", err);
            return;
        }
    };
    for region in map {
        println!(
            "{:010x}-{:010x} {:?} attr {:#x}",
            region.base,
            region.base + region.size - 1,
            region.mem_type,
            region.attributes.0
        );
    }
    println!(
        "handoff: map at {:p}, {} entries; MiniOS ends here",
        map.as_ptr(),
        map.len()
    );
    unsafe {
        System::exit_minios();
    }
    loop {
        core::hint::spin_loop();
    }
}

/// Display information about the system.
pub fn cmd_about() {
    let info = System::boot_info();
    let memsize1 = MemoryManager::total_memory_size();
    let memsize2 = MemoryManager::total_extended_memory_size();
    println!("{} v{}", SYSTEM_NAME, CURRENT_VERSION,);
    if memsize2 > 0 {
        let memsize1 = (memsize1 + 0xfffff) >> 20;
        let memsize = memsize1 + memsize2;
        print!(
            "MEMORY {} GB ({} MB + {} MB)",
            (memsize + 0x3ff) >> 10,
            memsize1,
            memsize2,
        );
    } else {
        let memsize1 = (memsize1 + 0x3ff) >> 10;
        print!("MEMORY {} MB ({} KB)", (memsize1 + 0x3ff) >> 10, memsize1,);
    }
    println!(", PLATFORM {}", info.platform_type);
    mode_info();
}

fn mode_info() {
    let stdout = System::stdout();
    let current_console_mode = stdout.current_mode();
    if let Some(current_graphics_mode) = System::conctl().current_graphics_mode() {
        println!(
            "Console: {} x {}, Graphics: {} x {}, Pixel Format: {:?}",
            current_console_mode.columns,
            current_console_mode.rows,
            current_graphics_mode.info.width,
            current_graphics_mode.info.height,
            current_graphics_mode.info.pixel_format
        );
    } else {
        println!(
            "Console: {} x {}, Text Mode",
            current_console_mode.columns, current_console_mode.rows
        );
    }
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

pub fn mode<'a>(mut args: impl Iterator<Item = &'a str>) {
    match args.next() {
        None => {}
        Some(arg) => {
            if arg.starts_with("i") {
                mode_info();
                return;
            } else if arg.starts_with("t") || arg.starts_with("T") {
                System::conctl().set_text_mode();
                mode_info();
                return;
            } else if arg.starts_with("g") || arg.starts_with("G") {
                if let Some(mode) = System::conctl().preferred_graphics_mode() {
                    let _ = System::conctl().set_graphics_mode(mode);
                } else {
                    let _ = System::conctl().set_graphics_mode_from_list(&[
                        // PreferredGraphicsMode::new(1920, 1080, PixelFormat::BGRX8888),
                        // PreferredGraphicsMode::new(1280, 720, PixelFormat::BGRX8888),
                        // PreferredGraphicsMode::new(800, 600, PixelFormat::BGRX8888),
                        // PreferredGraphicsMode::new(800, 600, PixelFormat::Indexed8),
                        PreferredGraphicsMode::new(640, 480, PixelFormat::Indexed8),
                        PreferredGraphicsMode::new(320, 200, PixelFormat::Indexed8),
                    ]);
                }
                mode_info();
                return;
            } else {
                println!("Unknown mode argument: {}", arg);
                return;
            }
        }
    }

    let mut exit_flag = false;
    loop {
        let stdout = System::stdout();
        stdout.reset();
        stdout.enable_cursor(false);

        let scr_size = Size::new(
            stdout.current_mode().columns as i32,
            stdout.current_mode().rows as i32,
        );

        let mut title_bar = TuiWindowBufferA::new(
            Rect::new(Point::new(0, 0), Size::new(scr_size.width, 1)),
            Inset::default(),
            TuiAttribute(0xf0),
        );
        // title_bar.put_string_at(Point::new(1, 0), SYSTEM_NAME, title_bar.default_attr);
        title_bar.draw_simple_title(SYSTEM_NAME, None, title_bar.default_attr);
        title_bar.draw_to(stdout);

        #[allow(unused_mut)]
        let mut status_bar = TuiWindowBufferA::new(
            Rect::new(
                Point::new(0, scr_size.height - 1),
                Size::new(scr_size.width, 1),
            ),
            Inset::default(),
            TuiAttribute(0xf0),
        );
        // status_bar.fill_rect(
        //     status_bar.bounds(),
        //     AsciiExt::from_char('@').unwrap(),
        //     status_bar.default_attr,
        // );
        status_bar.draw_to(stdout);

        let menu_items = [
            MainMenuItem::Start,
            MainMenuItem::TextMode,
            MainMenuItem::GraphicsMode,
        ];

        let mut menu_window = TuiWindowBufferA::new(
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
                                if let Some(mode) = System::conctl().preferred_graphics_mode() {
                                    let _ = System::conctl().set_graphics_mode(mode);
                                } else {
                                    let _ = System::conctl().set_graphics_mode_from_list(&[
                                        // PreferredGraphicsMode::new(1920, 1080, PixelFormat::BGRX8888),
                                        // PreferredGraphicsMode::new(1280, 720, PixelFormat::BGRX8888),
                                        // PreferredGraphicsMode::new(800, 600, PixelFormat::BGRX8888),
                                        // PreferredGraphicsMode::new(800, 600, PixelFormat::Indexed8),
                                        PreferredGraphicsMode::new(640, 480, PixelFormat::Indexed8),
                                        PreferredGraphicsMode::new(320, 200, PixelFormat::Indexed8),
                                    ]);
                                }
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
            System::stdout().reset();
            break;
        }
    }

    mode_info();
    return;
}
