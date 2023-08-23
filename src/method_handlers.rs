use std::{
    io,
    path::{Path, PathBuf},
    sync::Arc,
};

use tokio::fs;
use tracing::debug;

use crate::{
    config::Config,
    request::{HttpRequest, RequestType},
    response::Response,
};

fn resolve_path(conf: &Arc<Config>, uri: &[u8]) -> PathBuf {
    let mut path = conf.root_dir.join(Path::new(
        &String::from_utf8_lossy(if uri.starts_with(b"/") {
            &uri[1..]
        } else {
            uri
        })
        .as_ref(),
    ));

    if path.is_dir() {
        path.push(conf.get_default_file());
    }

    debug!("Resolved path: {:?}", path);

    path
}

pub async fn handle_request<T: HttpRequest>(req: &T, conf: &Arc<Config>) -> Response {
    let resolved_path = resolve_path(conf, req.get_uri().as_ref().unwrap());
    match req.get_type().expect("Tried to handle invalid request") {
        RequestType::Get => Response::from_io_result(
            handle_get(&resolved_path).await,
            resolved_path
                .extension()
                .map(|s| s.to_str().expect("Failed to convert from OsStr"))
                .unwrap_or("txt"),
        ),
        RequestType::Delete => todo!(),
        RequestType::Put => todo!(),
        RequestType::Head => todo!(),
    }
}

async fn handle_get(resolved_path: &Path) -> io::Result<Vec<u8>> {
    fs::read(resolved_path).await
}
