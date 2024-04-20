use std::fmt::Debug;

use raptor_core::request::{HttpRequest, RequestError, RequestType};

pub struct Request<'buf> {
    ty: RequestType,
    uri: &'buf [u8],
}

impl Debug for Request<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!(
            "{{type: {:?}, uri: {}}}",
            self.ty,
            String::from_utf8_lossy(self.uri)
        ))
    }
}

impl<'headers, 'buffer> TryFrom<httparse::Request<'headers, 'buffer>> for Request<'buffer> {
    type Error = RequestError<'buffer>;

    fn try_from(value: httparse::Request<'headers, 'buffer>) -> Result<Self, Self::Error> {
        let ty = RequestType::try_from(
            value
                .method
                .map(|s| s.as_bytes())
                .ok_or(RequestError::MalformedRequest)?,
        )?;

        Ok(Self {
            ty,
            uri: value
                .path
                .map(|s| s.as_bytes())
                .ok_or(RequestError::MalformedRequest)?,
        })
    }
}

impl<'buf> HttpRequest for Request<'buf> {
    fn get_type(&self) -> Result<RequestType, RequestError> {
        Ok(self.ty)
    }

    fn get_uri(&self) -> Result<&[u8], RequestError> {
        Ok(self.uri)
    }
}
