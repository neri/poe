// File System
// Most of them are clones of Rust's original definition.

use megstd::io::{Read, Result, Write};
use megstd::path::*;
use megstd::*;

pub struct File {
    //
}

impl File {
    pub fn create<P: AsRef<Path>>(_path: P) -> Result<File> {
        Err(io::ErrorKind::PermissionDenied.into())
    }

    pub fn open<P: AsRef<Path>>(_path: P) -> Result<File> {
        Err(io::ErrorKind::NotFound.into())
    }

    pub fn sync_all(&self) -> Result<()> {
        todo!()
    }

    pub fn sync_data(&self) -> Result<()> {
        todo!()
    }

    pub fn set_len(&self, _size: u64) -> Result<()> {
        todo!()
    }

    pub fn try_clone(&self) -> Result<File> {
        todo!()
    }

    pub fn set_permissions(&self, _perm: Permissions) -> Result<()> {
        todo!()
    }
}

impl Read for File {
    fn read(&mut self, _buf: &mut [u8]) -> Result<usize> {
        todo!()
    }
}

impl Write for File {
    fn write(&mut self, _buf: &[u8]) -> Result<usize> {
        todo!()
    }

    fn flush(&mut self) -> Result<()> {
        todo!()
    }
}

pub fn read_dir<P: AsRef<Path>>(_path: P) -> Result<ReadDir> {
    todo!()
}

pub fn canonicalize<P: AsRef<Path>>(_path: P) -> Result<PathBuf> {
    todo!()
}

#[derive(Debug, Clone)]
pub struct Metadata {
    //
}

impl Metadata {
    pub fn file_type(&self) -> FileType {
        todo!()
    }

    #[inline]
    pub fn is_dir(&self) -> bool {
        self.file_type().is_dir()
    }

    #[inline]
    pub fn is_file(&self) -> bool {
        self.file_type().is_file()
    }

    pub fn len(&self) -> u64 {
        todo!()
    }

    pub fn permissions(&self) -> Permissions {
        todo!()
    }

    // pub fn modified(&self) -> Result<SystemTime>
    // pub fn accessed(&self) -> Result<SystemTime>
    // pub fn created(&self) -> Result<SystemTime>
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct FileType(());

impl FileType {
    pub fn is_dir(&self) -> bool {
        todo!()
    }

    pub fn is_file(&self) -> bool {
        todo!()
    }

    pub fn is_symlink(&self) -> bool {
        todo!()
    }

    pub fn is_block_device(&self) -> bool {
        todo!()
    }

    pub fn is_char_device(&self) -> bool {
        todo!()
    }

    pub fn is_fifo(&self) -> bool {
        todo!()
    }

    pub fn is_socket(&self) -> bool {
        todo!()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Permissions(usize);

impl Permissions {
    pub fn readonly(&self) -> bool {
        todo!()
    }

    pub fn set_readonly(&mut self, _readonly: bool) {
        todo!()
    }
}

pub struct ReadDir(());

impl Iterator for ReadDir {
    type Item = Result<DirEntry>;

    fn next(&mut self) -> Option<Result<DirEntry>> {
        todo!()
        // self.0.next().map(|entry| entry.map(DirEntry))
    }
}

pub struct DirEntry(());

impl DirEntry {
    pub fn path(&self) -> PathBuf {
        todo!()
    }

    pub fn metadata(&self) -> Result<Metadata> {
        todo!()
    }

    pub fn file_type(&self) -> Result<FileType> {
        todo!()
    }

    pub fn file_name(&self) -> OsString {
        todo!()
    }
}
