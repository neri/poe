//! Fujitsu FM TOWNS
//!
//! # NOTE
//!
//! May not work or may need to be adjusted as it has not been fully verified on actual hardware.
//!

mod fmt_kbd;
mod fmt_text;

use crate::mem::{MemoryManager, MemoryType};
use crate::platform::x86_pc::pic::Irq;
use crate::*;
use x86::isolated_io::{LoIoPortRB, LoIoPortWB};

pub(super) unsafe fn init(_info: &BootInfo) {
    unsafe {
        fmt_text::FmtText::init();

        // 4000_0000-7fff_ffff io space
        // 8000_0000-bfff_ffff video subsystem
        // c000_0000-ffff_ffff rom space
        MemoryManager::register_memmap(0x4000_0000..0x1_0000_0000, MemoryType::Reserved).unwrap();

        arch::vm86::VM86::init();

        super::pic::Pic::init(
            0x00,
            0x02,
            0x10,
            0x12,
            0b00011001,
            0x07,
            0b0001_1101,
            0b0000_1001,
            0,
            [
                0x40, 0x41, 0x42, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48, 0x49, 0x4a, 0x4b, 0x4c, 0x4d,
                0x4e, 0x4f,
            ],
        );

        super::pit::Pit::init(0x0040, 0x0044, 0x0046, 307, Irq(0), timer_irq_handler);
        LoIoPortWB::<0x60>::new().write(0x81);
        Hal::cpu().enable_interrupt();

        fmt_kbd::FmtKbd::init();

        // Irq(11).register(irq11).unwrap();

        // loop {
        //     Hal::cpu().wait_for_interrupt();
        // }
    }
}

pub(super) unsafe fn exit() {
    // TODO:
}

fn timer_irq_handler(_irq: Irq) {
    super::pit::Pit::advance_tick();
    unsafe {
        let mut al = LoIoPortRB::<0x60>::new().read();
        al = (al >> 2) | 0x80;
        LoIoPortWB::<0x60>::new().write(al);
    }
}

// pub fn wait_us(us: usize) {
//     for _ in 0..us {
//         unsafe {
//             asm!("out 0x6c, al");
//         }
//     }
// }

// struct MousePosition {
//     x: u16,
//     y: u16,
// }

// static mut MOUSE_POSITION: MousePosition = MousePosition { x: 0, y: 0 };

// fn irq11(_irq: Irq) {
//     unsafe {
//         let port_com = 0x04d6;
//         let port_b = 0x4d2;
//         let com_off = 0b0000_1111u8;
//         let com_on = 0b0010_1111u8;

//         asm!("out dx, al", in("edx") port_com, in("al") com_off);
//         wait_us(1);

//         asm!("out dx, al", in("edx") port_com, in("al") com_on);
//         wait_us(80);
//         let p0: u8;
//         asm!("in al, dx", in("edx") port_b, out("al") p0);
//         wait_us(20);

//         asm!("out dx, al", in("edx") port_com, in("al") com_off);
//         wait_us(40);
//         let p1: u8;
//         asm!("in al, dx", in("edx") port_b, out("al") p1);
//         wait_us(10);

//         asm!("out dx, al", in("edx") port_com, in("al") com_on);
//         wait_us(40);
//         let p2: u8;
//         asm!("in al, dx", in("edx") port_b, out("al") p2);
//         wait_us(10);

//         asm!("out dx, al", in("edx") port_com, in("al") com_off);
//         wait_us(40);
//         let p3: u8;
//         asm!("in al, dx", in("edx") port_b, out("al") p3);

//         let x = 0 - (((p0 & 0x0F) << 4) | (p1 & 0x0F)) as i8 as i16;
//         let y = 0 - (((p2 & 0x0F) << 4) | (p3 & 0x0F)) as i8 as i16;

//         let shared = &mut *(&raw mut MOUSE_POSITION);
//         let ix = (shared.x as i16 + x).clamp(0, 639);
//         let iy = (shared.y as i16 + y).clamp(0, 399);
//         shared.x = ix as u16;
//         shared.y = iy as u16;

//         // notify mouse move to tsugaru
//         // TOWNSIO_VM_HOST_IF_CMD_STATUS=0x2386,
//         // TOWNSIO_VM_HOST_IF_DATA=      0x2387,
//         // TOWNS_VMIF_CMD_NOTIFY_MOUSE = 0x0A, // Notify Mouse Position
//         asm!("out dx, al", in("edx") 0x2387, in("al") shared.x as u8);
//         asm!("out dx, al", in("edx") 0x2387, in("al") (shared.x >> 8) as u8);
//         asm!("out dx, al", in("edx") 0x2387, in("al") shared.y as u8);
//         asm!("out dx, al", in("edx") 0x2387, in("al") (shared.y >>8) as u8);
//         asm!("out dx, al", in("edx") 0x2386, in("al") 0x0au8);

//         println!(
//             "M {:02X} {:02X} {:02X} {:02X} {:03} {:03}",
//             p0, p1, p2, p3, shared.x, shared.y
//         );
//     }
// }
