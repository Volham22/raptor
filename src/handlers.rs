use std::str::FromStr;

use http::{Response, StatusCode, Uri};
use httparse::Request;
use tokio::io::{AsyncRead, AsyncWrite};

use crate::{
    config::Vhost,
    stream_utils::{serialize_header, write_buffered_read_to_stream, write_bytes_to_stream},
};

use self::{
    get::{handle_get, GetBody},
    head::handle_head,
};

/// Handle the http request and return if the connection should be kept with the client (http header 'Connection')
pub async fn handle_request<'headers, 'buffer, T: AsyncRead + AsyncWrite + std::marker::Unpin>(
    req: &Request<'headers, 'buffer>,
    tcp_stream: &mut T,
    vhosts_list: &[Vhost],
) -> Result<bool, String> {
    let uri = Uri::from_str(req.path.unwrap()).unwrap_or(Uri::default());
    if let Ok(vhost) = select_vhost(vhosts_list, req) {
        match req.method.unwrap() {
            "GET" => handle_get(uri, tcp_stream, &vhost.root_dir).await?,
            "HEAD" => handle_head(tcp_stream).await?,
            _ => return Err(format!("Unsupported method {}", req.method.unwrap())),
        };
    } else {
        send_invalid_request(tcp_stream).await?;
    }

    Ok(should_keep_connection(req))
}

fn should_keep_connection<'headers, 'buffer>(req: &Request<'headers, 'buffer>) -> bool {
    req.headers
        .iter()
        .any(|h| h.name.to_lowercase() == "connection" && h.value == b"keep-alive")
}

fn select_vhost<'headers, 'buffer, 'vhosts>(
    vhosts_list: &'vhosts [Vhost],
    req: &Request<'headers, 'buffer>,
) -> Result<&'vhosts Vhost, String> {
    if let Some(req_host) = req.headers.iter().find(|h| h.name.to_lowercase() == "host") {
        if let Some(matched_vhost) = vhosts_list.iter().find(|vh| {
            fnmatch_regex::glob_to_regex(&vh.name)
                .unwrap()
                .is_match(req_host.name)
        }) {
            Ok(matched_vhost)
        } else {
            // If none of the vhosts matched we take the first one from the list that is marked as default vhost
            // if there's no vhost marked as default in the config file the first vhost is returned.
            // Note that vhosts are in the same order as they're declared in the config file.
            if let Some(default_vhost) =
                vhosts_list.iter().find(|vh| vh.is_default.unwrap_or(false))
            {
                Ok(default_vhost)
            } else {
                Ok(&vhosts_list[0])
            }
        }
    } else {
        Err("Invalid request.".to_string())
    }
}

async fn send_invalid_request<T: AsyncRead + AsyncWrite + std::marker::Unpin>(
    tcp_stream: &mut T,
) -> Result<(), String> {
    let body_bytes = b"Bad request.";
    let mut resp = Response::builder()
        .status(StatusCode::BAD_REQUEST)
        .header("Content-length", body_bytes.len())
        .body(GetBody::Bytes(body_bytes.to_vec()))
        .unwrap();

    send_response(&mut resp, tcp_stream).await
}

pub(self) async fn send_response<T: AsyncRead + AsyncWrite + std::marker::Unpin>(
    resp: &mut Response<GetBody>,
    tcp_stream: &mut T,
) -> Result<(), String> {
    let serialized_header = serialize_header(resp);

    write_bytes_to_stream(tcp_stream, &serialized_header).await?;

    match resp.body_mut() {
        GetBody::Bytes(b) => write_bytes_to_stream(tcp_stream, b).await,
        GetBody::File(f) => write_buffered_read_to_stream(tcp_stream, f).await,
    }
}

mod get {
    use std::{io::ErrorKind, os::unix::prelude::MetadataExt};

    use http::{Response, StatusCode, Uri};
    use tokio::{
        fs::{metadata, File},
        io::{AsyncRead, AsyncWrite},
    };

    use super::send_response;

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

    pub async fn handle_get<T: AsyncRead + AsyncWrite + std::marker::Unpin>(
        uri: Uri,
        tcp_stream: &mut T,
        server_root: &str,
    ) -> Result<(), String> {
        let metadata = metadata(server_root.to_owned() + uri.path()).await;

        let mut response = match metadata {
            Ok(result) => {
                if result.is_file() {
                    // println!("File: {}", ".".to_owned() + uri.path());
                    let f = File::open(server_root.to_owned() + uri.path())
                        .await
                        .unwrap();

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
            send_response(&mut response, tcp_stream).await?;
            Err(response.body().try_into_str().unwrap())
        } else {
            send_response(&mut response, tcp_stream).await
        }
    }
}

mod head {
    use http::{Response, StatusCode};
    use tokio::io::{AsyncRead, AsyncWrite};

    use crate::stream_utils::{serialize_header, write_bytes_to_stream};

    pub async fn handle_head<T: AsyncRead + AsyncWrite + std::marker::Unpin>(
        tcp_stream: &mut T,
    ) -> Result<(), String> {
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
