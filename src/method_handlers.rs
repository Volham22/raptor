use std::{io, path::Path};

use tokio::fs;
use tracing::debug;

use crate::{
    request::{HttpRequest, RequestType},
    response::Response,
};

pub async fn handle_request<T: HttpRequest>(req: &T, root_dir: &Path) -> Response {
    match req.get_type().expect("Tried to handle invalid request") {
        RequestType::Get => Response::from_io_result(
            handle_get(req, root_dir).await,
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

async fn handle_get<T: HttpRequest>(req: &T, root_dir: &Path) -> io::Result<Vec<u8>> {
    let uri = req.get_uri().unwrap();
    let path = root_dir.join(Path::new(
        &String::from_utf8_lossy(if uri.starts_with(b"/") {
            &uri[1..]
        } else {
            uri
        })
        .as_ref(),
    ));
    debug!("Path: {:?}", path);

    fs::read(path).await
}
