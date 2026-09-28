pub mod class;
mod control;
#[cfg(feature = "device_tree")]
pub mod dwc2;
#[cfg(test)]
pub mod fake;
pub mod hcd;
pub mod input;
pub mod inventory;
pub mod log;
pub mod manager;
mod storage;
pub mod xhci;

pub use manager::UsbManager;
