use bytes::Bytes;

use super::FrameError;

#[derive(Debug)]
pub struct Continuation {
    pub headers: Vec<(Bytes, Bytes)>,
    is_end_header: bool,
}

impl Continuation {
    pub fn from_bytes(
        value: &[u8],
        decoder: &mut hpack::Decoder,
        flags: u8,
        length: usize,
    ) -> Result<Self, FrameError> {
        if value.len() < length {
            return Err(FrameError::BadFrameSize);
        }

        let payload_data = decoder
            .decode(&value[..length])
            .map_err(FrameError::HpackDecoderError)?;

        Ok(Self {
            is_end_header: flags & 0x04 > 0,
            headers: payload_data
                .into_iter()
                .map(|(k, v)| (Bytes::from(k), Bytes::from(v)))
                .collect(),
        })
    }

    fn is_end_header(&self) -> bool {
        self.is_end_header
    }
}
