use libusb::SetupPacket;

pub const CLASS_HID: u8 = 3;
pub const SUBCLASS_BOOT: u8 = 1;
pub const PROTOCOL_KEYBOARD: u8 = 1;

pub const fn set_protocol_boot(interface: u8) -> SetupPacket {
    SetupPacket::new(0x21, 0x0b, 0, interface as u16, 0)
}
/// Idle duration in the 4 ms units the HID specification uses.
///
/// Zero means "report only when something changes", which is what a BIOS
/// asks for. That makes every report irreplaceable: a low-speed keyboard
/// behind a transaction translator loses one whenever a complete-split fails
/// to collect it, and the device will never send it again because nothing has
/// changed since. Asking for a duration shorter than the poll interval makes
/// the device restate the current keys on every poll instead, so a lost
/// report costs latency rather than a keystroke. Repeats do not turn into
/// duplicate key presses: `BootKeyboard` only reports usages that were not
/// already held.
pub const IDLE_DURATION_4MS: u8 = 2;

pub const fn set_idle(interface: u8, duration_4ms: u8) -> SetupPacket {
    SetupPacket::new(0x21, 0x0a, (duration_4ms as u16) << 8, interface as u16, 0)
}
