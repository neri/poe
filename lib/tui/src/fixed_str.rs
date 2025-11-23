use core::ops::Deref;

/// Fixed-size string buffer.
pub struct FixedStrBuf<const N: usize> {
    buf: [u8; N],
    len: usize,
}

impl<const N: usize> FixedStrBuf<N> {
    /// Creates a new, empty `FixedStrBuf`.
    #[inline]
    pub const fn new() -> Self {
        Self {
            buf: [0; N],
            len: 0,
        }
    }

    #[inline]
    pub const fn capacity(&self) -> usize {
        N
    }

    /// Returns the current length of the string.
    #[inline]
    pub const fn len(&self) -> usize {
        self.len
    }

    /// Clears the string, removing all contents.
    pub fn clear(&mut self) {
        self.len = 0;
    }

    /// Returns a slice of the internal buffer.
    #[inline]
    pub const fn as_slice(&self) -> &[u8] {
        // SAFETY: The buffer is always valid up to `self.len`.
        unsafe { core::slice::from_raw_parts(self.buf.as_ptr(), self.len) }
    }

    /// Returns the string as a `&str`.
    #[inline]
    pub const fn as_str(&self) -> &str {
        // SAFETY: We ensure that only valid UTF-8 is written to the buffer.
        unsafe { core::str::from_utf8_unchecked(self.as_slice()) }
    }

    /// Pushes a character to the end of the string.
    pub fn push(&mut self, c: char) -> Result<(), ()> {
        let mut buf = [0u8; 4];
        let encoded = c.encode_utf8(&mut buf);
        let encoded_len = encoded.len();
        if self.len + encoded_len > N {
            return Err(());
        }
        for &b in &buf[..encoded_len] {
            self.buf[self.len] = b;
            self.len += 1;
        }
        Ok(())
    }
}

impl<const N: usize> Deref for FixedStrBuf<N> {
    type Target = str;

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}
