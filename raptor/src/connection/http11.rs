use std::{io, sync::Arc};

use format_bytes::write_bytes;
use raptor_core::{config::Config, method_handlers::handle_request, response::Response};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};
use tokio_rustls::server::TlsStream;
use tracing::{debug, error, event, info, trace, Level};

use crate::http11;

async fn send_response(mut stream: TlsStream<TcpStream>, response: Response) -> io::Result<()> {
    const CRLF: &[u8; 2] = b"\r\n";
    event!(Level::TRACE, "Sending response");

    let mut buffer: [u8; 128] = [0; 128];
    let mut slice = &mut buffer[..];
    write_bytes!(
        &mut slice,
        b"HTTP/1.1 {} {}\r\n",
        response.code,
        response.get_code_bytes()
    )?;

    stream.write_all(&buffer).await?;

    for (hd_name, hd_value) in response.headers {
        stream.write_all(&hd_name).await?;
        stream.write_all(b": ").await?;
        stream.write_all(&hd_value).await?;
        stream.write_all(CRLF).await?;
    }

    stream.write_all(CRLF).await?;

    if let Some(body) = response.body.as_ref() {
        stream.write_all(body).await?;
    }

    // Make sure the request is fully sent
    stream.flush().await?;

    Ok(())
}

#[tracing::instrument(level = "info")]
pub async fn do_http11(mut stream: TlsStream<TcpStream>, config: Arc<Config>) -> io::Result<()> {
    let mut buffer = [0u8; 1024];
    event!(Level::TRACE, "Started HTTP/1.1 handling");

    loop {
        let read_size = stream.read(&mut buffer).await?;
        let mut headers = [httparse::EMPTY_HEADER; 64];
        let mut req = httparse::Request::new(&mut headers);

        match req.parse(&buffer) {
            Ok(parsed) => match parsed {
                httparse::Status::Complete(_) => {
                    let req = http11::Request::try_from(req).unwrap();
                    info!("Got request: {:?}", req);
                    let response = handle_request(&req, &config).await;
                    debug!("Send response: {:?}", response);
                    send_response(stream, response).await?;

                    break;
                }
                httparse::Status::Partial if read_size == 0 => {
                    if read_size == 0 {
                        debug!("Received 0 bytes. Reached EOF.");
                        trace!("Send bad request response.");
                        send_response(stream, Response::bad_request()).await?;
                        break;
                    }
                }
                httparse::Status::Partial => {
                    trace!("Request partially received keep reading...");
                    debug!("Got: {:?}", &buffer);
                    continue;
                }
            },
            Err(err) => {
                error!("Failed to parse http request: {}", err);
                trace!("Replying bad request and closing connection!");
                send_response(stream, Response::bad_request()).await?;
                break; // We're closing connection after bad request.
            }
        }
    }

    event!(
        Level::TRACE,
        "Connection handling finished. Closing connection"
    );
    Ok(())
}
