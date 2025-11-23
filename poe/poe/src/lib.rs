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
    let _ = System::conctl().set_graphics_mode_from_list(&[
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
        use tui::{TuiAttribute, buffer::*, coord::*};

        let mut window = TuiWindowBufferAscii::new(
            Rect::new(Point::new(2, 2), Size::new(20, 10)),
            Inset::new(2, 2, 2, 2),
            TuiAttribute(0xf0),
        );

        window.draw_simple_title("Hello", TuiAttribute(0x9f), TuiAttribute(0x0f));
        window.put_string_at(Point::new(2, 2), "Hello, world!", window.default_attr);
        window.put_text(Point::new(2, 4), "Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat. Duis aute irure dolor in reprehenderit in voluptate velit esse cillum dolore eu fugiat nulla pariatur. Excepteur sint occaecat cupidatat non proident, sunt in culpa qui officia deserunt mollit anim id est laborum.", TuiAttribute(0x07), 0);

        window.draw_to(stdout);
    }

    // #[rustfmt::skip]
    // let logo = [
    //     "()=() |",
    //     "('Y') <",
    //     "q . p |",
    //     "()_()  "
    // ];
    // let mut logo = logo.iter();

    // println!("");
    // println!("  {}", logo.next().unwrap());
    // println!("  {} poe poe poe~", logo.next().unwrap());
    // println!("  {}", logo.next().unwrap());
    // println!("  {}", logo.next().unwrap());
    // println!("");

    // #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    // if false {
    //     if let Some(fdt) = System::device_tree() {
    //         println!("DEVICE TREE:");
    //         println!("  Model: {}", fdt.root().model());
    //         // println!("  Compatible: {}", fdt.root().compatible());
    //         println!("");

    //         if true {
    //             dump_fdt_node(fdt.root(), 0);
    //             println!("");
    //         }
    //     }
    // }

    stdout.set_attribute(0xb0);
    println!("");
    println!("");

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
            println!(
                "Critical fatal error!!!\nUnable to execute command: {:?}",
                line
            );
        }
    }
}

#[allow(dead_code)]
fn dump_fdt_node(node: &fdt::Node, level: usize) {
    use fdt::*;

    if let Some(compatible) = node.get_prop_str(PropName::COMPATIBLE) {
        println!(
            "{}{} ({:?})",
            "  ".repeat(level),
            node.name().as_str(),
            compatible,
        );
    } else {
        println!("{}{}", "  ".repeat(level), node.name().as_str(),);
    }

    for prop in node.props() {
        match prop.name() {
            PropName::COMPATIBLE => {
                for _ in 0..level {
                    print!("  ");
                }
                print!("  {} <", prop.name().as_str());
                for (i, s) in prop.string_list().enumerate() {
                    if i > 0 {
                        print!(" {:?}", s);
                    } else {
                        print!("{:?}", s);
                    }
                }
                println!(">");
            }
            PropName::REG => {
                let reg = node.reg().unwrap();
                for reg in reg {
                    for _ in 0..level {
                        print!("  ");
                    }
                    if reg.1 > 0 {
                        println!("  reg <{:#010x} {:#010x}>", reg.0, reg.1,);
                    } else {
                        println!("  reg <{:#010x}>", reg.0,);
                    }
                }
            }
            PropName::ADDRESS_CELLS
            | PropName::SIZE_CELLS
            | PropName::INTERRUPT_CELLS
            | PropName::INTERRUPT_PARENT
            | PropName::CLOCK_CELLS
            | PropName::PHANDLE => {
                for _ in 0..level {
                    print!("  ");
                }
                println!(
                    "  {} <{:#x}>",
                    prop.name().as_str(),
                    prop.as_u32().unwrap_or_default()
                );
            }
            PropName("linux,initrd-end")
            | PropName("linux,initrd-start")
            | PropName::CLOCK_FREQUENCY
            | PropName::TIMEBASE_FREQUENCY => {
                for _ in 0..level {
                    print!("  ");
                }
                println!(
                    "  {} <{:#x}>",
                    prop.name().as_str(),
                    prop.as_u32().unwrap_or_default()
                );
            }
            PropName::DEVICE_TYPE | PropName::MODEL | PropName::NAME | PropName::STATUS => {
                for _ in 0..level {
                    print!("  ");
                }
                println!("  {} <{:?}>", prop.name().as_str(), prop.as_str());
            }
            _ => {
                for _ in 0..level {
                    print!("  ");
                }
                print!("  {} <", prop.name().as_str());

                let bytes = prop.bytes();
                let len = bytes.len();
                if len > 0 {
                    let maybe_words = (len & 3) == 0;
                    let mut maybe_asciz = false;
                    if bytes[len - 1] == 0 {
                        maybe_asciz = true;
                        for i in 0..len - 1 {
                            if bytes[i] < 0x20 || bytes[i] > 0x7e {
                                maybe_asciz = false;
                                break;
                            }
                        }
                    }

                    if maybe_asciz || !maybe_words {
                        print!("\"");
                        for c in bytes {
                            match *c {
                                0 => {
                                    print!("\\0");
                                }
                                0x20..=0x7E => {
                                    print!("{}", *c as char);
                                }
                                _ => {
                                    print!("\\x{:02x}", *c);
                                }
                            }
                        }
                        print!("\"");
                    } else {
                        let words = unsafe {
                            core::slice::from_raw_parts(bytes.as_ptr() as *const BeU32, len / 4)
                        };
                        for (i, w) in words.iter().enumerate() {
                            if i > 0 {
                                print!(" {:#x}", w.as_u32());
                            } else {
                                print!("{:#x}", w.as_u32());
                            }
                        }
                    }
                }
                println!(">");
            }
        }
    }

    for child in node.children() {
        dump_fdt_node(&child, level + 1);
    }
}
