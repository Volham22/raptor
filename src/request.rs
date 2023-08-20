use thiserror::Error;
use tracing::error;

#[derive(Error, Debug, Clone, PartialEq)]
pub enum RequestError<'a> {
    #[error("Unsupported method {0:?}")]
    UnsupportedMethod(&'a [u8]),
    #[error("Malformed request")]
    MalformedRequest,
}

pub enum RequestType {
    Get,
    Delete,
    Put,
    Head,
}

impl<'a> TryFrom<&'a [u8]> for RequestType {
    type Error = RequestError<'a>;

    fn try_from(value: &'a [u8]) -> Result<Self, Self::Error> {
        match value.to_ascii_lowercase().as_slice() {
            b"get" => Ok(Self::Get),
            b"delete" => Ok(Self::Delete),
            b"put" => Ok(Self::Put),
            b"head" => Ok(Self::Head),
            _ => {
                error!("Unsupported method: {value:#02x?}");
                Err(RequestError::UnsupportedMethod(value))
            }
        }
    }
}

pub trait HttpRequest {
    fn get_type(&self) -> Result<RequestType, RequestError>;
    fn get_uri(&self) -> Result<&[u8], RequestError>;
    fn get_payload(&self) -> Option<&[u8]> {
        None
    }
}
