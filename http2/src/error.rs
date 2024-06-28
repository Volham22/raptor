use thiserror::Error;

pub type Http2Result<T> = Result<T, Http2Error>;

#[derive(Debug, Error)]
pub enum Http2Error {
    #[error("HTTP2 error: {0:?}")]
    Http2(h2::Error),
}
