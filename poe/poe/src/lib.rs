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
    println!("()=()  | {} v{}", SYSTEM_NAME, CURRENT_VERSION,);
    println!("('Y') <  PLATFORM: {}", info.platform);
    print!("q . p  | MEMORY: ");
    if memsize2 > 0 {
        let memsize1 = (memsize1 + 0xfffff) >> 20;
        let memsize = memsize1 + memsize2;
        println!(
            "{} GB ({} MB + {} MB)",
            (memsize + 0x3ff) >> 10,
            memsize1,
            memsize2,
        );
    } else {
        let memsize1 = (memsize1 + 0x3ff) >> 10;
        println!("{} MB ({} KB)", (memsize1 + 0x3ff) >> 10, memsize1,);
    }
    println!("()_()");
    println!("");

    if false {
        println!("Memory Map:");
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
    }

    loop {
        print!(">");
        if let Some(line) = System::line_input(16) {
            if line.is_empty() {
                continue;
            }
            println!("{:?}?", line);
        }
    }
}
