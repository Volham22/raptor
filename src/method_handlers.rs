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

/// Check if the resolved path is `root_dir` or a child. If not we're outside
/// the server root_dir. It's propably a path traversal attack attempt.
async fn check_for_path_traversal(conf: &Arc<Config>, resolved_path: &Path) -> io::Result<bool> {
    let absolute_path = fs::canonicalize(resolved_path).await?;
    debug!("Absolute path: {:?}", absolute_path);
    Ok(!absolute_path.starts_with(&conf.root_dir))
}

pub async fn handle_request<T: HttpRequest>(req: &T, conf: &Arc<Config>) -> Response {
    let resolved_path = resolve_path(conf, req.get_uri().as_ref().unwrap());

    match check_for_path_traversal(conf, &resolved_path).await {
        Ok(true) => {
            return Response::from_io_result(
                Err(io::ErrorKind::PermissionDenied.into()),
                resolved_path.to_str().unwrap(),
            );
        }
        Ok(false) => (),
        Err(err) => {
            return Response::from_io_result(Err(err), resolved_path.to_str().unwrap());
        }
    };

    match req.get_type().expect("Tried to handle invalid request") {
        RequestType::Get => Response::from_io_result(
            handle_get(&resolved_path).await.map(Some),
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
