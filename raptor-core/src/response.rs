use std::io;

use tracing::error;

const DATE_FMT_STR: &str = "%a, %d %b %Y %H:%M:%S GMT";
const SERVER_NAME: &[u8; 6] = b"raptor";

#[derive(Debug)]
pub struct Response {
    pub code: u16,
    pub headers: Vec<(Vec<u8>, Vec<u8>)>,
    pub body: Option<Vec<u8>>,
}

impl Response {
    pub fn bad_request() -> Self {
        Self {
            code: 400,
            headers: vec![
                (
                    b"date".to_vec(),
                    format!("{}", chrono::Utc::now().format(DATE_FMT_STR))
                        .as_bytes()
                        .to_vec(),
                ),
                (b"content-length".to_vec(), b"0".to_vec()),
                (b"server".to_vec(), SERVER_NAME.to_vec()),
            ],
            body: None,
        }
    }

    pub fn from_io_result(value: io::Result<Option<Vec<u8>>>, extension: &str) -> Self {
        match value {
            Ok(Some(body)) => Self {
                code: 200,
                headers: vec![
                    (
                        b"content-length".to_vec(),
                        body.len().to_string().as_bytes().to_vec(),
                    ),
                    (
                        b"content-type".to_vec(),
                        mime_guess::from_ext(extension)
                            .first_raw()
                            .unwrap_or("text/plain")
                            .as_bytes()
                            .to_vec(),
                    ),
                    (
                        b"date".to_vec(),
                        format!("{}", chrono::Utc::now().format(DATE_FMT_STR))
                            .as_bytes()
                            .to_vec(),
                    ),
                    (b"server".to_vec(), SERVER_NAME.to_vec()),
                ],
                body: Some(body),
            },
            Ok(None) => Self {
                code: 200,
                headers: vec![
                    (
                        b"content-type".to_vec(),
                        mime_guess::from_ext(extension)
                            .first_raw()
                            .unwrap_or("text/plain")
                            .as_bytes()
                            .to_vec(),
                    ),
                    (
                        b"date".to_vec(),
                        format!("{}", chrono::Utc::now().format(DATE_FMT_STR))
                            .as_bytes()
                            .to_vec(),
                    ),
                    (b"server".to_vec(), SERVER_NAME.to_vec()),
                ],
                body: None,
            },
            Err(err) => match err.kind() {
                io::ErrorKind::NotFound => Self {
                    code: 404,
                    headers: vec![
                        (
                            b"date".to_vec(),
                            format!("{}", chrono::Utc::now().format(DATE_FMT_STR))
                                .as_bytes()
                                .to_vec(),
                        ),
                        (b"content-length".to_vec(), b"0".to_vec()),
                        (b"server".to_vec(), SERVER_NAME.to_vec()),
                    ],
                    body: None,
                },
                io::ErrorKind::PermissionDenied => Self {
                    code: 403,
                    headers: vec![
                        (
                            b"date".to_vec(),
                            format!("{}", chrono::Utc::now().format(DATE_FMT_STR))
                                .as_bytes()
                                .to_vec(),
                        ),
                        (b"server".to_vec(), SERVER_NAME.to_vec()),
                        (b"content-length".to_vec(), b"0".to_vec()),
                    ],
                    body: None,
                },
                _ => Self {
                    code: 500,
                    headers: vec![
                        (
                            b"date".to_vec(),
                            format!("{}", chrono::Utc::now().format(DATE_FMT_STR))
                                .as_bytes()
                                .to_vec(),
                        ),
                        (b"server".to_vec(), SERVER_NAME.to_vec()),
                        (b"content-length".to_vec(), b"0".to_vec()),
                    ],
                    body: None,
                },
            },
        }
    }

    pub fn get_code_bytes(&self) -> &[u8] {
        match self.code {
            200 => b"OK",
            400 => b"Bad Request",
            401 => b"Unauthorized",
            403 => b"Forbidden",
            404 => b"Not Found",
            _ => {
                error!("Unknown http code: {}", self.code);
                b"Internal Server Error"
            }
        }
    }
}

impl From<Response> for http::Response<()> {
    fn from(value: Response) -> Self {
        let mut result = http::Response::builder().status(value.code);

        for (k, v) in value.headers.into_iter() {
            result = result.header(k, v);
        }

        result.body(()).expect("unreachable")
    }
}
