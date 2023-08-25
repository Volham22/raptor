use std::mem;

use crate::http2::response::ResponseSerialize;

use super::{ErrorType, FrameError};

#[derive(Debug, Copy, Clone)]
pub struct ResetStream {
    pub error_code: ErrorType,
}

impl ResetStream {
    pub fn new(error_code: u32) -> Self {
        Self {
            error_code: error_code.into(),
        }
    }

    pub fn from_bytes(data: &[u8], length: usize) -> Result<Self, FrameError> {
        if data.len() < length {
            return Err(FrameError::BadFrameSize(data.len()));
        }

        Ok(Self {
            error_code: u32::from_be_bytes(<[u8; 4]>::try_from(&data[..4]).expect("unreachable"))
                .into(),
        })
    }
}

impl ResponseSerialize for ResetStream {
    fn serialize_response(&self, _: Option<&mut hpack::Encoder>) -> Vec<u8> {
        (self.error_code as u32).to_be_bytes().to_vec()
    }

    fn compute_frame_length(&self, _: Option<&mut hpack::Encoder>) -> u32 {
        mem::size_of::<u32>() as u32
    }
}
