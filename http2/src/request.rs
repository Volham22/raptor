use raptor_core::request::{HttpRequest, RequestError, RequestType};

pub struct Http2Request<'a, T>(&'a http::Request<T>);

impl<'a, T> Http2Request<'a, T> {
    pub fn new(req: &'a http::Request<T>) -> Self {
        Self(req)
    }
}

impl<'a, T> HttpRequest for Http2Request<'a, T> {
    fn get_type(&self) -> Result<RequestType, RequestError> {
        Ok(match *self.0.method() {
            http::Method::GET => RequestType::Get,
            http::Method::DELETE => RequestType::Delete,
            http::Method::PUT => RequestType::Put,
            http::Method::HEAD => RequestType::Head,
            _ => Err(RequestError::UnsupportedMethod(
                self.0.method().as_str().as_bytes(),
            ))?,
        })
    }

    fn get_uri(&self) -> Result<&[u8], RequestError> {
        self.0
            .uri()
            .path_and_query()
            .map(|pq| pq.as_str().as_bytes())
            .ok_or(RequestError::MalformedRequest)
    }
}
