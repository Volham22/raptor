use std::io;

use bytes::Bytes;

const DATE_FMT_STR: &str = "%a, %d %b %Y %H:%M:%S GMT";
const SERVER_NAME: &[u8; 6] = b"raptor";

pub struct Response {
    pub code: u16,
    pub headers: Vec<(Bytes, Bytes)>,
    pub body: Option<Vec<u8>>,
}

impl Response {
    pub fn from_io_result(value: io::Result<Vec<u8>>, extension: &str) -> Self {
        match value {
            Ok(body) => Self {
                code: 200,
                headers: vec![
                    (
                        Bytes::from_static(b"content-length"),
                        Bytes::copy_from_slice(body.len().to_string().as_bytes()),
                    ),
                    (
                        Bytes::from_static(b"content-type"),
                        Bytes::from(
                            mime_guess::from_ext(extension)
                                .first_raw()
                                .unwrap_or("text/plain")
                                .as_bytes(),
                        ),
                    ),
                    (
                        Bytes::from_static(b"date"),
                        Bytes::copy_from_slice(
                            format!("{}", chrono::Utc::now().format(DATE_FMT_STR)).as_bytes(),
                        ),
                    ),
                    (
                        Bytes::from_static(b"server"),
                        Bytes::from_static(SERVER_NAME),
                    ),
                ],
                body: Some(body),
            },
            Err(err) => match err.kind() {
                io::ErrorKind::NotFound => Self {
                    code: 404,
                    headers: vec![
                        (
                            Bytes::from_static(b"date"),
                            Bytes::copy_from_slice(
                                format!("{}", chrono::Utc::now().format(DATE_FMT_STR)).as_bytes(),
                            ),
                        ),
                        (
                            Bytes::from_static(b"server"),
                            Bytes::from_static(SERVER_NAME),
                        ),
                    ],
                    body: None,
                },
                io::ErrorKind::PermissionDenied => Self {
                    code: 403,
                    headers: vec![
                        (
                            Bytes::from_static(b"date"),
                            Bytes::copy_from_slice(
                                format!("{}", chrono::Utc::now().format(DATE_FMT_STR)).as_bytes(),
                            ),
                        ),
                        (
                            Bytes::from_static(b"server"),
                            Bytes::from_static(SERVER_NAME),
                        ),
                    ],
                    body: None,
                },
                _ => Self {
                    code: 500,
                    headers: vec![
                        (
                            Bytes::from_static(b"date"),
                            Bytes::copy_from_slice(
                                format!("{}", chrono::Utc::now().format(DATE_FMT_STR)).as_bytes(),
                            ),
                        ),
                        (
                            Bytes::from_static(b"server"),
                            Bytes::from_static(SERVER_NAME),
                        ),
                    ],
                    body: None,
                },
            },
        }
    }
}
