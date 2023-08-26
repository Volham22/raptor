use std::mem;

use crate::http2::response::ResponseSerialize;

use super::FrameError;

pub const PING_LENGTH: usize = mem::size_of::<u64>();

pub struct Ping {
    pub opaque_data: u64,
    pub is_ack: bool,
}

impl Default for Ping {
    fn default() -> Self {
        Ping {
            opaque_data: 666,
            is_ack: false,
        }
    }
}

impl Ping {
    pub fn new(is_ack: bool, opaque_data: u64) -> Self {
        Self {
            opaque_data,
            is_ack,
        }
    }

    pub fn from_bytes(data: &[u8], flags: u8) -> Result<Self, FrameError> {
        if data.len() < PING_LENGTH {
            return Err(FrameError::BadFrameSize(data.len()));
        }

        Ok(Self {
            opaque_data: u64::from_be_bytes(
                <[u8; 8]>::try_from(&data[..PING_LENGTH]).expect("unreachable"),
            ),
            is_ack: flags & 0x01 > 0,
        })
    }
}

impl ResponseSerialize for Ping {
    fn get_flags(&self) -> u8 {
        if self.is_ack {
            0x01
        } else {
            0x00
        }
    }

    fn serialize_response(&self, _: Option<&mut hpack::Encoder>) -> Vec<u8> {
        self.opaque_data.to_be_bytes().to_vec()
    }

    fn compute_frame_length(&self, _: Option<&mut hpack::Encoder>) -> u32 {
        PING_LENGTH as u32
    }
}
