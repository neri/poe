//! Mini Libraries
#![cfg_attr(not(test), no_std)]

pub mod fixedvec;
pub mod rand;
pub mod unknown_enum;

#[cfg(test)]
mod tests;
