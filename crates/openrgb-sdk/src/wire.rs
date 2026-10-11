//! Little-endian cursor helpers for the OpenRGB wire format.
//!
//! Strings on the wire are `u16 len` (len includes the NUL terminator) followed
//! by the bytes; the v6 `configuration` field uses a `u32` length instead.

use crate::error::{Error, Result};

#[derive(Debug)]
pub struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    pub fn remaining(&self) -> usize {
        self.buf.len() - self.pos
    }

    pub fn position(&self) -> usize {
        self.pos
    }

    pub fn bytes(&mut self, n: usize) -> Result<&'a [u8]> {
        if self.remaining() < n {
            return Err(Error::Truncated { needed: n, available: self.remaining() });
        }
        let slice = &self.buf[self.pos..self.pos + n];
        self.pos += n;
        Ok(slice)
    }

    pub fn u8(&mut self) -> Result<u8> {
        Ok(self.bytes(1)?[0])
    }

    pub fn u16(&mut self) -> Result<u16> {
        let b = self.bytes(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    pub fn u32(&mut self) -> Result<u32> {
        let b = self.bytes(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    pub fn i32(&mut self) -> Result<i32> {
        Ok(self.u32()? as i32)
    }

    /// `u16` length-prefixed string (length includes the NUL terminator).
    pub fn str16(&mut self) -> Result<String> {
        let len = self.u16()? as usize;
        Ok(cstr_to_string(self.bytes(len)?))
    }

    /// `u32` length-prefixed string (length includes the NUL terminator).
    pub fn str32(&mut self) -> Result<String> {
        let len = self.u32()? as usize;
        Ok(cstr_to_string(self.bytes(len)?))
    }
}

/// Decode a NUL-terminated (or NUL-padded) byte string.
pub fn cstr_to_string(bytes: &[u8]) -> String {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).into_owned()
}

/// Encode a string as NUL-terminated bytes (used for whole-payload strings).
pub fn cstr(s: &str) -> Vec<u8> {
    let mut v = Vec::with_capacity(s.len() + 1);
    v.extend_from_slice(s.as_bytes());
    v.push(0);
    v
}

#[derive(Debug, Default)]
pub struct Writer {
    buf: Vec<u8>,
}

impl Writer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_capacity(n: usize) -> Self {
        Self { buf: Vec::with_capacity(n) }
    }

    pub fn len(&self) -> usize {
        self.buf.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }

    pub fn u8(&mut self, v: u8) -> &mut Self {
        self.buf.push(v);
        self
    }

    pub fn u16(&mut self, v: u16) -> &mut Self {
        self.buf.extend_from_slice(&v.to_le_bytes());
        self
    }

    pub fn u32(&mut self, v: u32) -> &mut Self {
        self.buf.extend_from_slice(&v.to_le_bytes());
        self
    }

    pub fn i32(&mut self, v: i32) -> &mut Self {
        self.buf.extend_from_slice(&v.to_le_bytes());
        self
    }

    pub fn raw(&mut self, bytes: &[u8]) -> &mut Self {
        self.buf.extend_from_slice(bytes);
        self
    }

    /// `u16` length (including NUL) + bytes + NUL.
    pub fn str16(&mut self, s: &str) -> &mut Self {
        self.u16((s.len() + 1) as u16);
        self.buf.extend_from_slice(s.as_bytes());
        self.buf.push(0);
        self
    }

    /// `u32` length (including NUL) + bytes + NUL.
    pub fn str32(&mut self, s: &str) -> &mut Self {
        self.u32((s.len() + 1) as u32);
        self.buf.extend_from_slice(s.as_bytes());
        self.buf.push(0);
        self
    }

    /// Overwrite a `u32` at `pos`; used to back-patch `data_size` prefixes.
    pub fn patch_u32(&mut self, pos: usize, v: u32) {
        self.buf[pos..pos + 4].copy_from_slice(&v.to_le_bytes());
    }

    pub fn finish(self) -> Vec<u8> {
        self.buf
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_scalars_and_strings() {
        let mut w = Writer::new();
        w.u8(7).u16(0xBEEF).u32(0xDEADBEEF).i32(-2).str16("hi").str32("");
        let buf = w.finish();

        let mut r = Reader::new(&buf);
        assert_eq!(r.u8().unwrap(), 7);
        assert_eq!(r.u16().unwrap(), 0xBEEF);
        assert_eq!(r.u32().unwrap(), 0xDEADBEEF);
        assert_eq!(r.i32().unwrap(), -2);
        assert_eq!(r.str16().unwrap(), "hi");
        assert_eq!(r.str32().unwrap(), "");
        assert_eq!(r.remaining(), 0);
    }

    #[test]
    fn truncated_read_is_an_error() {
        let mut r = Reader::new(&[1, 2, 3]);
        assert!(matches!(r.u32(), Err(Error::Truncated { needed: 4, available: 3 })));
    }

    #[test]
    fn cstr_stops_at_nul() {
        assert_eq!(cstr_to_string(b"abc\0junk"), "abc");
        assert_eq!(cstr_to_string(b"no-nul"), "no-nul");
        assert_eq!(cstr("x"), vec![b'x', 0]);
    }
}
