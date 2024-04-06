use crate::config::Config;
use std::io;
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio_rustls::TlsAcceptor;
use tracing::{debug, error, info, trace};

mod http11;
mod http2;

pub use http2::*;

pub async fn do_connection(
    ssl_socket: TlsAcceptor,
    client_socket: TcpStream,
    config: Arc<Config>,
) -> io::Result<()> {
    let stream = ssl_socket.accept(client_socket).await?;
    let (_, conn) = stream.get_ref();

    match conn.alpn_protocol() {
        Some(b"h2") => {
            debug!("Using 'h2' protocol");
            http2::do_http2(stream, config).await
        }
        Some(b"http/1.1") => {
            debug!("Using 'http/1.1' protocol");
            http11::do_http11(stream, config).await
        }
        Some(protocol) => {
            error!("Bad protocol received: '{:?}' closing connection", protocol);
            Ok(())
        }
        None => {
            info!("No protocol negociated via APLN! Using HTTP/1.1 as default.");
            trace!("Does the client support it?");
            http11::do_http11(stream, config).await
        }
    }
}
