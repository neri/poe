//! Pre-OS Execution Environment
#![no_std]
#![no_main]

extern crate alloc;
use libminios::{mem::MemoryManager, prelude::*};

pub use libminios::prelude;

static SYSTEM_NAME: &str = "POE";

static CURRENT_VERSION: Version = Version::new(0, 0, 0, "");

pub fn main() {
    let info = System::boot_info();
    let memsize1 = MemoryManager::total_memory_size();
    let memsize2 = MemoryManager::total_extended_memory_size();

    let stdout = System::stdout();
    stdout.set_attribute(0);
    stdout.clear_screen();

    #[rustfmt::skip]
    let logo = [
        "()=() |", 
        "('Y') <", 
        "q . p |", 
        "()_()  "
    ];
    let mut logo = logo.iter();

    println!("{}", logo.next().unwrap());
    println!("{} poe poe poe~", logo.next().unwrap());
    println!("{}", logo.next().unwrap());
    println!("{}", logo.next().unwrap());
    println!("");

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
    println!(", PLATFORM {}", info.platform);
    println!("");

    if true {
        println!("Memory Map:");
        stdout.set_attribute(0x78);
        for item in MemoryManager::memory_list() {
            let range = item.range();
            println!(
                "  {:08x} - {:08x}: {:?}",
                range.start,
                range.end - 1,
                item.mem_type
            )
        }
        println!("");
        stdout.set_attribute(0);
    }

    println!("POE super-shell v0.0");
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
