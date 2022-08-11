use std::str::FromStr;

use http::Uri;
use httparse::Request;
use tokio::net::TcpStream;

use self::{get::handle_get, head::handle_head};

pub async fn handle_request<'headers, 'buffer>(
    req: &Request<'headers, 'buffer>,
    tcp_stream: &mut TcpStream,
) -> Result<(), String> {
    let uri = Uri::from_str(req.path.unwrap()).unwrap_or(Uri::default());
    match req.method.unwrap() {
        "GET" => handle_get(uri, tcp_stream).await,
        "HEAD" => handle_head(tcp_stream).await,
        _ => Err(format!("Unsupported method {}", req.method.unwrap())),
    }
}

mod get {
    use std::{io::ErrorKind, os::unix::prelude::MetadataExt};

    use http::{Response, StatusCode, Uri};
    use tokio::{
        fs::{metadata, File},
        net::TcpStream,
    };

    use crate::stream_utils::{
        serialize_header, write_buffered_read_to_stream, write_bytes_to_stream,
    };

    pub enum GetBody {
        File(File),
        Bytes(Vec<u8>),
    }

    impl GetBody {
        fn try_into_str(&self) -> Option<String> {
            match self {
                GetBody::File(_) => None,
                GetBody::Bytes(b) => Some(String::from_utf8_lossy(b.as_slice()).to_string()),
            }
        }
    }

    pub async fn handle_get(uri: Uri, tcp_stream: &mut TcpStream) -> Result<(), String> {
        let metadata = metadata(".".to_owned() + uri.path()).await;

        let mut response = match metadata {
            Ok(result) => {
                if result.is_file() {
                    println!("File: {}", ".".to_owned() + uri.path());
                    let f = File::open(".".to_owned() + uri.path()).await.unwrap();

                    Response::builder()
                        .status(StatusCode::OK)
                        .header("Content-Length", result.size())
                        .body(GetBody::File(f))
                        .unwrap()
                } else if result.is_dir() {
                    Response::builder()
                        .status(StatusCode::NOT_IMPLEMENTED)
                        .body(GetBody::Bytes(b"Not implemented yet.".to_vec()))
                        .unwrap()
                } else {
                    Response::builder()
                        .status(StatusCode::INTERNAL_SERVER_ERROR)
                        .body(GetBody::Bytes(b"Oops, something went wrong :(".to_vec()))
                        .unwrap()
                }
            }
            Err(msg) => match msg.kind() {
                ErrorKind::NotFound => Response::builder()
                    .status(StatusCode::NOT_FOUND)
                    .body(GetBody::Bytes(b"Resource not found!".to_vec()))
                    .unwrap(),
                ErrorKind::PermissionDenied => Response::builder()
                    .status(StatusCode::FORBIDDEN)
                    .body(GetBody::Bytes(
                        b"Not allowed to access this ressource!".to_vec(),
                    ))
                    .unwrap(),
                _ => Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .body(GetBody::Bytes(b"Oops, something went wrong :(".to_vec()))
                    .unwrap(),
            },
        };

        if response.status() != StatusCode::OK {
            send_get_response(&mut response, tcp_stream).await?;
            Err(response.body().try_into_str().unwrap())
        } else {
            send_get_response(&mut response, tcp_stream).await
        }
    }

    async fn send_get_response(
        resp: &mut Response<GetBody>,
        tcp_stream: &mut TcpStream,
    ) -> Result<(), String> {
        let serialized_header = serialize_header(resp);

        write_bytes_to_stream(tcp_stream, &serialized_header).await?;

        match resp.body_mut() {
            GetBody::Bytes(b) => write_bytes_to_stream(tcp_stream, b).await,
            GetBody::File(f) => write_buffered_read_to_stream(tcp_stream, f).await,
        }
    }
}

mod head {
    use http::{Response, StatusCode};
    use tokio::net::TcpStream;

    use crate::stream_utils::{serialize_header, write_bytes_to_stream};

    pub async fn handle_head(tcp_stream: &mut TcpStream) -> Result<(), String> {
        let resp = Response::builder()
            .status(StatusCode::OK)
            .header("Content-Length", 0)
            .body(())
            .unwrap();

        let content = serialize_header(&resp);
        write_bytes_to_stream(tcp_stream, &content).await?;
        Ok(())
    }
}
