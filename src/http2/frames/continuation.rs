use bytes::Bytes;

use super::FrameError;

#[derive(Debug)]
pub struct Continuation {
    pub headers: Bytes,
    is_end_header: bool,
}

impl Continuation {
    pub fn from_bytes(value: &[u8], flags: u8, length: usize) -> Result<Self, FrameError> {
        if value.len() < length {
            return Err(FrameError::BadFrameSize);
        }

        Ok(Self {
            is_end_header: flags & 0x04 > 0,
            headers: Bytes::copy_from_slice(&value[..length]),
        })
    }

    pub fn is_end_header(&self) -> bool {
        self.is_end_header
    }
}
