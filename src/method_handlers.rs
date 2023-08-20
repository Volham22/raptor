use std::{io, path::Path};

use tokio::fs;
use tracing::debug;

use crate::{
    request::{HttpRequest, RequestType},
    response::Response,
};

pub async fn handle_request<T: HttpRequest>(req: &T) -> Response {
    match req.get_type().expect("Tried to handle invalid request") {
        RequestType::Get => Response::from_io_result(
            handle_get(req).await,
            Path::new(String::from_utf8_lossy(req.get_uri().unwrap()).as_ref())
                .extension()
                .map(|s| s.to_str().unwrap())
                .unwrap_or("txt"),
        ),
        RequestType::Delete => todo!(),
        RequestType::Put => todo!(),
        RequestType::Head => todo!(),
    }
}

async fn handle_get<T: HttpRequest>(req: &T) -> io::Result<Vec<u8>> {
    let path = format!(".{}", String::from_utf8_lossy(req.get_uri().unwrap()));
    let get_path = Path::new(&path);
    debug!("Path: {:?}", get_path);

    fs::read(get_path).await
}
