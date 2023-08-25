use bytes::Bytes;

use crate::http2::response::ResponseSerialize;

#[derive(Debug)]
pub struct Data {
    payload: Bytes,
    pub flags: u8,
}

impl Data {
    pub fn new(payload: Bytes) -> Self {
        Self { payload, flags: 0 }
    }

    // pub fn is_end_stream(&self) -> bool {
    //     self.flags & 0x01 > 0
    // }
    //
    // pub fn is_padded(&self) -> bool {
    //     self.flags & 0x08 > 0
    // }

    pub fn set_flags(&mut self, flags: u8) {
        self.flags = flags;
    }
}

impl ResponseSerialize for Data {
    fn serialize_response(&self, _: Option<&mut hpack::Encoder>) -> Vec<u8> {
        self.payload.to_vec()
    }

    fn compute_frame_length(&self, _: Option<&mut hpack::Encoder>) -> u32 {
        self.payload.len() as u32
    }

    fn get_flags(&self) -> u8 {
        self.flags
    }
}
