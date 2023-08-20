use std::{io, path::Path};

use tokio::fs;

use crate::request::{HttpRequest, RequestType};

pub async fn handle_request<T: HttpRequest>(req: &T) -> io::Result<Option<Vec<u8>>> {
    match req.get_type().expect("Tried to handle invalid request") {
        RequestType::Get => handle_get(req).await.map(Some),
        RequestType::Delete => todo!(),
        RequestType::Put => todo!(),
        RequestType::Head => todo!(),
    }
}

pub async fn handle_get<T: HttpRequest>(req: &T) -> io::Result<Vec<u8>> {
    let path = format!(".{}", String::from_utf8_lossy(req.get_uri().unwrap()));
    let get_path = Path::new(&path);
    println!("Path: {:?}", get_path);

    fs::read(get_path).await
}
