use std::collections::HashMap;

use bytes::Bytes;

use crate::request::{HttpRequest, RequestError, RequestType};

#[derive(Debug)]
pub struct Headers(HashMap<Bytes, Bytes>);

impl Headers {
    pub fn from_bytes(
        value: &[u8],
        decoder: &mut hpack::Decoder,
        flags: u8,
        length: usize,
    ) -> Result<Self, &'static str> {
        if Self::is_padded(flags) || Self::is_priority(flags) {
            todo!("Padding and priority stream are not implemented");
        }

        if !Self::is_end_header(flags) {
            todo!("Header continuation is not implemented");
        }

        match decoder.decode(&value[..length]) {
            Ok(hds) => Ok(Self(
                hds.iter()
                    .map(|(k, v)| {
                        (
                            Bytes::copy_from_slice(k.as_slice()),
                            Bytes::copy_from_slice(v.as_slice()),
                        )
                    })
                    .collect(),
            )),
            Err(err) => {
                eprintln!("HPACK decoder error: {:?}", err);
                Err("decoder error")
            }
        }
    }

    // #[inline]
    // fn is_end_stream(flags: u8) -> bool {
    //     flags & 0x01 > 0
    // }

    #[inline]
    fn is_end_header(flags: u8) -> bool {
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
        match self.0.get(b":method".as_slice()) {
            Some(kind) => RequestType::try_from(&kind[..]),
            None => Err(RequestError::MalformedRequest),
        }
    }

    fn get_uri(&self) -> Result<&[u8], RequestError> {
        self.0
            .get(b":path".as_slice())
            .map(|v| &v[..])
            .ok_or(RequestError::MalformedRequest)
    }
}
