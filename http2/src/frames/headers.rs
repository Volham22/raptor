use crate::{server::ConnectionStream, utils};

use super::{
    errors::{FrameError, FrameResult},
    Frame,
};

#[derive(Debug)]
pub(crate) struct Headers {
    flags: u8,
    pub headers: Vec<(Vec<u8>, Vec<u8>)>,
}

impl Headers {
    pub async fn receive_header_frame(
        stream: &mut ConnectionStream,
        frame: &Frame,
        hpack_decoder: &mut fluke_hpack::Decoder<'_>,
    ) -> FrameResult<Self> {
        let frame_bytes = utils::receive_n_bytes(stream, frame.length as usize)
            .await
            .map_err(FrameError::IOError)?;

        Self::parse_header_frame(frame, hpack_decoder, &frame_bytes)
    }

    pub(self) fn parse_header_frame(
        frame: &Frame,
        hpack_decoder: &mut fluke_hpack::Decoder<'_>,
        frame_bytes: &[u8],
    ) -> FrameResult<Self> {
        if frame.flags & 0x08 > 0 {
            todo!("Padding isn't supported yet");
        }

        if frame.flags & (0x01 << 6) > 0 {
            todo!("Priority is not supported yet");
        }

        let headers = hpack_decoder
            .decode(frame_bytes)
            .map_err(FrameError::HpackDecodeError)?;

        Ok(Self {
            flags: frame.flags,
            headers,
        })
    }

    #[inline]
    pub fn has_end_stream(&self) -> bool {
        self.flags & 0x01 > 0
    }

    #[inline]
    pub fn has_end_headers(&self) -> bool {
        self.flags & (0x01 << 2) > 0
    }
}

#[cfg(test)]
mod tests {
    use crate::frames::{headers::Headers, Frame, FrameType, FRAME_HEADER_SIZE};

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
        let result =
            Headers::parse_header_frame(&FRAME, &mut decoder, &HEADER_BYTES[FRAME_HEADER_SIZE..]);
        let Ok(frame) = result else {
            panic!("Should be correct: {:?}", result.unwrap_err());
        };

        assert!(!frame.has_end_stream());
        assert!(frame.has_end_headers());

        for ((key, value), (expected_key, expected_value)) in
            frame.headers.iter().zip(expected_headers)
        {
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
            length: 49,
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
        let result =
            Headers::parse_header_frame(&FRAME, &mut decoder, &HEADER_BYTES[FRAME_HEADER_SIZE..]);
        let Ok(frame) = result else {
            panic!("Should be correct: {:?}", result.unwrap_err());
        };

        assert!(frame.has_end_stream());
        assert!(frame.has_end_headers());

        for ((key, value), (expected_key, expected_value)) in
            frame.headers.iter().zip(expected_headers)
        {
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
