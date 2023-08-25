use std::mem;

use crate::http2::response::ResponseSerialize;

pub struct ResetStream {
    pub error_code: u32,
}

impl ResetStream {
    pub fn new(error_code: u32) -> Self {
        Self { error_code }
    }
}

impl ResponseSerialize for ResetStream {
    fn serialize_response(&self, encoder: Option<&mut hpack::Encoder>) -> Vec<u8> {
        self.error_code.to_be_bytes().to_vec()
    }

    fn compute_frame_length(&self, encoder: Option<&mut hpack::Encoder>) -> u32 {
        mem::size_of::<u32>() as u32
    }
}
