use std::sync::Arc;

use raptor_core::request::{self, HttpRequest};
use tokio::sync::Mutex;
use tracing::{trace, warn};

use crate::{server::ConnectionStream, utils};

use super::{
    continuation::Continuation,
    errors::{FrameError, FrameResult},
    Frame, SerializeFrame,
};

pub(crate) type Header = (Vec<u8>, Vec<u8>);

#[derive(Debug, Clone)]
pub(crate) enum HeaderStatus {
    Complete(Vec<Header>),
    Partial(Vec<u8>),
}

#[derive(Debug, Clone)]
pub(crate) struct Headers {
    flags: u8,
    pub headers: HeaderStatus,
}

impl Headers {
    pub fn new(flags: u8, headers: HeaderStatus) -> Self {
        Self { flags, headers }
    }

    pub async fn receive_header_frame(
        stream: &mut ConnectionStream,
        frame: &Frame,
        hpack_decoder: &Arc<Mutex<fluke_hpack::Decoder<'_>>>,
    ) -> FrameResult<Self> {
        trace!("Receiving header frame");
        let frame_bytes = utils::receive_n_bytes(stream, frame.length as usize)
            .await
            .map_err(FrameError::IOError)?;
        let mut hpack_decoder = hpack_decoder.lock().await;

        Self::parse_header_frame(frame, &mut hpack_decoder, frame_bytes)
    }

    pub(self) fn parse_header_frame(
        frame: &Frame,
        hpack_decoder: &mut fluke_hpack::Decoder<'_>,
        frame_bytes: Vec<u8>,
    ) -> FrameResult<Self> {
        if frame.flags & 0x08 > 0 {
            todo!("Padding isn't supported yet");
        }

        // TODO: Support priority
        let has_priority = frame.flags & 0x20 > 0;
        if has_priority {
            warn!("Priority is not supported yet. Ignoring");
        }

        let payload_offset = if has_priority { 5usize } else { 0 };

        Ok(Self {
            flags: frame.flags,
            headers: if frame.flags & 0x04 > 0 {
                HeaderStatus::Complete(
                    hpack_decoder
                        .decode(&frame_bytes[payload_offset..frame.length as usize])
                        .map_err(FrameError::HpackDecodeError)?,
                )
            } else {
                HeaderStatus::Partial(frame_bytes[payload_offset..].to_vec())
            },
        })
    }

    pub(crate) fn consume_contination(&mut self, continuation: &Continuation) {
        match &mut self.headers {
            HeaderStatus::Partial(data) => {
                data.extend_from_slice(&continuation.data);
            }
            _ => unreachable!(),
        }
    }

    pub(crate) async fn decode_headers(
        &mut self,
        decoder: &Arc<Mutex<fluke_hpack::Decoder<'_>>>,
    ) -> FrameResult<()> {
        let HeaderStatus::Partial(data) = &self.headers else {
            unreachable!();
        };
        let mut decoder = decoder.lock().await;
        let headers = decoder.decode(data).map_err(FrameError::HpackDecodeError)?;
        self.headers = HeaderStatus::Complete(headers);

        Ok(())
    }

    #[inline]
    #[cfg(test)]
    pub fn has_end_stream(&self) -> bool {
        self.flags & 0x01 > 0
    }

    #[inline]
    pub fn has_end_headers(&self) -> bool {
        self.flags & 0x04 > 0
    }
}

impl HttpRequest for Headers {
    fn get_type(&self) -> Result<request::RequestType, request::RequestError> {
        let HeaderStatus::Complete(headers) = &self.headers else {
            unreachable!("Reponse on partially received headers");
        };

        let method = headers
            .iter()
            .find(|(k, _)| k == b":method")
            .ok_or(request::RequestError::MalformedRequest)?;

        Ok(match method.1.as_slice() {
            b"GET" => request::RequestType::Get,
            b"PUT" => request::RequestType::Put,
            b"HEAD" => request::RequestType::Head,
            b"DELETE" => request::RequestType::Delete,
            _ => Err(request::RequestError::MalformedRequest)?,
        })
    }

    fn get_uri(&self) -> Result<&[u8], request::RequestError> {
        let HeaderStatus::Complete(headers) = &self.headers else {
            unreachable!("Reponse on partially received headers");
        };

        let (_, value) = headers
            .iter()
            .find(|(k, _)| k == b":path")
            .ok_or(request::RequestError::MalformedRequest)?;

        Ok(value)
    }
}

impl SerializeFrame for Headers {
    async fn serialize_frame(
        self,
        frame: &mut Frame,
        encoder: Option<Arc<Mutex<fluke_hpack::Encoder<'_>>>>,
    ) -> Vec<u8> {
        let encoder_header = encoder.expect("must be provided for header");
        let mut encoder = encoder_header.lock().await;
        let HeaderStatus::Complete(headers) = self.headers else {
            unreachable!("Reponse on partially received headers");
        };

        // Update frame
        frame.flags = self.flags;

        let encoded_bytes = encoder.encode(
            headers
                .iter()
                .map(|(k, v)| (k.as_slice(), v.as_slice()))
                .collect::<Vec<(&[u8], &[u8])>>(),
        );
        frame.flags = self.flags;
        frame.length = encoded_bytes.len() as u32;

        encoded_bytes
    }
}

#[cfg(test)]
mod tests {
    use crate::frames::{
        headers::{HeaderStatus, Headers},
        Frame, FrameType, FRAME_HEADER_SIZE,
    };

    #[test]
    fn parse_correct_header_response() {
        const HEADER_BYTES: &[u8; 205] = include_bytes!("../../tests/data/header_frame1.raw");
        const FRAME: Frame = Frame {
            length: 196,
            frame_type: FrameType::Header,
            flags: 0x04,
            stream_id: 1,
        };
        let expected_headers: [(Vec<u8>, Vec<u8>); 13] = [
            (b":status".to_vec(), b"200".to_vec()),
            (b"date".to_vec(), b"Sun, 12 Aug 2018 17:30:41 GMT".to_vec()),
            (b"content-type".to_vec(), b"text/plain".to_vec()),
            (
                b"last-modified".to_vec(),
                b"Tue, 08 May 2018 13:53:22 GMT".to_vec(),
            ),
            (b"etag".to_vec(), b"\"5af1abd2-3e\"".to_vec()),
            (b"accept-ranges".to_vec(), b"bytes".to_vec()),
            (b"content-length".to_vec(), b"62".to_vec()),
            (b"x-backend-header-rtt".to_vec(), b"0.002645".to_vec()),
            (b"server".to_vec(), b"nghttpx".to_vec()),
            (b"via".to_vec(), b"2 nghttpx".to_vec()),
            (b"x-frame-options".to_vec(), b"SAMEORIGIN".to_vec()),
            (b"x-xss-protection".to_vec(), b"1; mode=block".to_vec()),
            (b"x-content-type-options".to_vec(), b"nosniff".to_vec()),
        ];

        let mut decoder = fluke_hpack::Decoder::new();
        let result = Headers::parse_header_frame(
            &FRAME,
            &mut decoder,
            HEADER_BYTES[FRAME_HEADER_SIZE..].to_vec(),
        );
        let Ok(frame) = result else {
            panic!("Should be correct: {:?}", result.unwrap_err());
        };

        assert!(!frame.has_end_stream());
        assert!(frame.has_end_headers());

        let HeaderStatus::Complete(headers) = frame.headers else {
            panic!("Partial headers");
        };

        for ((key, value), (expected_key, expected_value)) in headers.iter().zip(expected_headers) {
            assert_eq!(
                key,
                &expected_key,
                "got: {} expected: {}",
                String::from_utf8_lossy(key),
                String::from_utf8_lossy(&expected_key)
            );
            assert_eq!(
                value,
                &expected_value,
                "got: {} expected: {}",
                String::from_utf8_lossy(value),
                String::from_utf8_lossy(&expected_value)
            );
        }
    }

    #[test]
    fn parse_correct_header_request() {
        const HEADER_BYTES: &[u8; 49] = include_bytes!("../../tests/data/header_frame2.raw");
        const FRAME: Frame = Frame {
            length: 40,
            frame_type: FrameType::Header,
            flags: 0x05,
            stream_id: 1,
        };
        let expected_headers: [(Vec<u8>, Vec<u8>); 6] = [
            (b":method".to_vec(), b"GET".to_vec()),
            (b":path".to_vec(), b"/humans.txt".to_vec()),
            (b":scheme".to_vec(), b"http".to_vec()),
            (b":authority".to_vec(), b"nghttp2.org".to_vec()),
            (b"user-agent".to_vec(), b"curl/7.61.0".to_vec()),
            (b"accept".to_vec(), b"*/*".to_vec()),
        ];

        let mut decoder = fluke_hpack::Decoder::new();
        let result = Headers::parse_header_frame(
            &FRAME,
            &mut decoder,
            HEADER_BYTES[FRAME_HEADER_SIZE..].to_vec(),
        );
        let Ok(frame) = result else {
            panic!("Should be correct: {:?}", result.unwrap_err());
        };

        assert!(frame.has_end_stream());
        assert!(frame.has_end_headers());

        let HeaderStatus::Complete(headers) = frame.headers else {
            panic!("Partial headers");
        };

        for ((key, value), (expected_key, expected_value)) in headers.iter().zip(expected_headers) {
            assert_eq!(
                key,
                &expected_key,
                "got: {} expected: {}",
                String::from_utf8_lossy(key),
                String::from_utf8_lossy(&expected_key)
            );
            assert_eq!(
                value,
                &expected_value,
                "got: {} expected: {}",
                String::from_utf8_lossy(value),
                String::from_utf8_lossy(&expected_value)
            );
        }
    }
}
