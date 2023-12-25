use std::{fmt::Write, io, sync::Arc};

use bytes::BytesMut;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};
use tokio_rustls::server::TlsStream;
use tracing::{debug, error, info, trace};

use crate::{config::Config, http11, method_handlers::handle_request, response::Response};

async fn send_response(mut stream: TlsStream<TcpStream>, response: Response) -> io::Result<()> {
    const CRLF: &[u8; 2] = b"\r\n";
    trace!("Sending response");

    let mut buffer = BytesMut::with_capacity(128);
    buffer
        .write_fmt(format_args!(
            "HTTP/1.1 {} {}\r\n",
            response.code,
            response.get_code_string()
        ))
        .expect("Failed to format response buffer");

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
    let mut buffer = BytesMut::with_capacity(1024);
    trace!("Started HTTP/1.1 handling");

    loop {
        let read_size = stream.read_buf(&mut buffer).await?;
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

    trace!("Connection handling finished. Closing connection");
    Ok(())
}
