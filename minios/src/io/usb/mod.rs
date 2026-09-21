pub mod class;
mod control;
#[cfg(test)]
pub mod fake;
pub mod hcd;
pub mod input;
pub mod log;
pub mod manager;
pub mod xhci;

pub use manager::UsbManager;
