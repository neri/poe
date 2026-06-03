//! FAT Filesystem Library
#![cfg_attr(not(test), no_std)]

extern crate alloc;

pub mod bpb;
pub mod dirent;

/// FAT Filesystem Type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FatType {
    /// FAT12, n_clusters < 4085
    Fat12,
    /// FAT16, 4085 <= n_clusters < 65525
    Fat16,
    /// FAT32, n_clusters >= 65525
    Fat32,
}
