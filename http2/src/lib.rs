use std::{io, sync::Arc};

use bytes::Bytes;
use error::{Http2Error, Http2Result};
use h2::{
    server::{self, SendResponse},
    RecvStream,
};
use http::Request;
use raptor_core::config::Config;
use request::Http2Request;
use tokio::net::TcpStream;
use tokio_rustls::server::TlsStream;
use tracing::instrument;

mod error;
mod request;

fn create_response(code: u16, headers: Vec<(Vec<u8>, Vec<u8>)>) -> http::Response<()> {
    let mut result = http::Response::builder().status(code);

    for (k, v) in headers.into_iter() {
        result = result.header(k, v);
    }

    result.body(()).expect("unreachable")
}

async fn handle_request(
    req: Request<RecvStream>,
    mut res: SendResponse<Bytes>,
    config: &Arc<Config>,
) -> Http2Result<()> {
    let h2_request = Http2Request::new(&req);
    let raptor_response = raptor_core::method_handlers::handle_request(&h2_request, config).await;
    if let Some(body) = raptor_response.body {
        let mut send = res
            .send_response(
                create_response(raptor_response.code, raptor_response.headers),
                false,
            )
            .map_err(Http2Error::Http2)?;

        let body_bytes: Bytes = body.into();
        send.send_data(body_bytes, true).map_err(Http2Error::Http2)
    } else {
        res.send_response(
            create_response(raptor_response.code, raptor_response.headers),
            true,
        )
        .map_err(Http2Error::Http2)?;
        Ok(())
    }
}

#[instrument(name = "http2_connection")]
pub async fn run_connection(stream: TlsStream<TcpStream>, config: Arc<Config>) -> io::Result<()> {
    let mut http2_conn = match server::handshake(stream).await {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("h2 error: {e}");
            return Ok(());
        }
    };

    while let Some(result) = http2_conn.accept().await {
        let task_conf = config.clone();
        tokio::spawn(async move {
            let (req, res) = result.map_err(Http2Error::Http2)?;
            handle_request(req, res, &task_conf).await
        });
    }

    Ok(())
}
