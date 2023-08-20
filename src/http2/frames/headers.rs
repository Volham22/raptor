use std::collections::HashMap;

use bytes::{BufMut, Bytes, BytesMut};
use tracing::{error, trace};

use crate::{
    http2::response::ResponseSerialize,
    request::{HttpRequest, RequestError, RequestType},
};

use super::FrameError;

#[derive(Debug, Clone)]
pub struct Headers {
    headers: HashMap<Bytes, Bytes>,
    end_stream: bool,
}

impl Headers {
    pub fn new(headers: &[(&[u8], &[u8])]) -> Self {
        Self {
            headers: headers
                .iter()
                .map(|(k, v)| {
                    (
                        Bytes::copy_from_slice(&k[..]),
                        Bytes::copy_from_slice(&v[..]),
                    )
                })
                .collect(),
            end_stream: false,
        }
    }

    pub fn extract_headers_data(
        value: &[u8],
        flags: u8,
        length: usize,
    ) -> Result<BytesMut, FrameError> {
        trace!("extracting header data");
        if length > value.len() {
            return Err(FrameError::BadFrameSize(value.len()));
        }

        if Self::is_padded(flags) {
            todo!("Padding and priority stream are not implemented");
        }

        let mut result = BytesMut::with_capacity(length);
        result.put(&value[..length]);

        Ok(result)
    }

    pub fn from_bytes(
        value: &[u8],
        decoder: &mut hpack::Decoder,
        flags: u8,
        length: usize,
    ) -> Result<Self, FrameError> {
        trace!("Decoding header frame");
        if length > value.len() {
            return Err(FrameError::BadFrameSize(value.len()));
        }

        if Self::is_padded(flags) {
            todo!("Padding and priority stream are not implemented");
        }

        let payload_offset = if Self::is_priority(flags) { 5 } else { 0 };
        match decoder.decode(&value[payload_offset..length]) {
            Ok(hds) => Ok(Self {
                headers: hds
                    .iter()
                    .map(|(k, v)| {
                        (
                            Bytes::copy_from_slice(k.as_slice()),
                            Bytes::copy_from_slice(v.as_slice()),
                        )
                    })
                    .collect(),
                end_stream: Self::is_end_stream(flags),
            }),
            Err(err) => {
                error!("HPACK decoder error: {:?}", err);
                Err(FrameError::HpackDecoderError(err))
            }
        }
    }

    pub fn should_stream_close(&self) -> bool {
        self.end_stream
    }

    #[inline]
    fn is_end_stream(flags: u8) -> bool {
        flags & 0x01 > 0
    }

    #[inline]
    pub fn is_end_header(flags: u8) -> bool {
        flags & 0x04 > 0
    }

    #[inline]
    fn is_padded(flags: u8) -> bool {
        flags & 0x08 > 0
    }

    #[inline]
    fn is_priority(flags: u8) -> bool {
        flags & 0x20 > 0
    }
}

impl HttpRequest for Headers {
    fn get_type(&self) -> Result<RequestType, RequestError> {
        match self.headers.get(b":method".as_slice()) {
            Some(kind) => RequestType::try_from(&kind[..]),
            None => Err(RequestError::MalformedRequest),
        }
    }

    fn get_uri(&self) -> Result<&[u8], RequestError> {
        self.headers
            .get(b":path".as_slice())
            .map(|v| &v[..])
            .ok_or(RequestError::MalformedRequest)
    }
}

impl ResponseSerialize for Headers {
    fn serialize_response(&self, encoder: Option<&mut hpack::Encoder>) -> Vec<u8> {
        encoder
            .unwrap()
            .encode(
                self.headers
                    .iter()
                    .filter(|(k, _)| k.starts_with(&[b':']))
                    .chain(self.headers.iter().filter(|(k, _)| !k.starts_with(&[b':'])))
                    .map(|(k, v)| (&k[..], &v[..]))
                    .collect::<Vec<(&[u8], &[u8])>>(),
            )
            .iter()
            .map(|x| x.to_be())
            .collect()
    }

    fn compute_frame_length(&self, encoder: Option<&mut hpack::Encoder>) -> u32 {
        encoder
            .unwrap()
            .encode(self.headers.iter().map(|(k, v)| (&k[..], &v[..])))
            .len() as u32
    }

    fn get_flags(&self) -> u8 {
        0x04
    }
}
