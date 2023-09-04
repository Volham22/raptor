use crate::config::Config;
use std::io;
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio_rustls::TlsAcceptor;
use tracing::{debug, error};

mod http11;
mod http2;

pub use http11::*;
pub use http2::*;

pub async fn do_connection(
    ssl_socket: TlsAcceptor,
    client_socket: TcpStream,
    config: Arc<Config>,
) -> io::Result<()> {
    let stream = ssl_socket.accept(client_socket).await?;
    let (_, conn) = stream.get_ref();
    debug!(
        "ALPN: {:?}",
        String::from_utf8_lossy(conn.alpn_protocol().unwrap())
    );

    match conn.alpn_protocol() {
        Some(b"h2") => http2::do_http2(stream, config).await,
        Some(b"http/1.1") => do_http11().await,
        Some(protocol) => {
            error!("Bad protocol received: '{:?}' closing connection", protocol);
            Ok(())
        }
        None => {
            error!("No protocol negociated via APLN! Closing connection");
            Ok(())
        }
    }
}
