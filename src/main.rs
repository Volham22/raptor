use std::{
    fs::File,
    io::{self, BufReader},
    net::ToSocketAddrs,
    path::Path,
    sync::Arc,
};

use connection::do_connection;
use rustls_pemfile::{certs, rsa_private_keys};
use tokio::net::TcpListener;
use tokio_rustls::{
    rustls::{self, Certificate, PrivateKey},
    TlsAcceptor,
};

mod connection;
mod http2;
mod request;
mod method_handlers;

fn load_certs(path: &Path) -> io::Result<Vec<Certificate>> {
    let mut cert_buffer = BufReader::new(File::open(path)?);

    certs(&mut cert_buffer)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid cert file"))
        .map(|mut certs| certs.drain(..).map(Certificate).collect())
}

fn load_keys(path: &Path) -> io::Result<Vec<PrivateKey>> {
    let mut key_buffer = BufReader::new(File::open(path)?);

    rsa_private_keys(&mut key_buffer)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid key file"))
        .map(|mut keys| keys.drain(..).map(PrivateKey).collect())
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let addr = "127.0.0.1:8000"
        .to_socket_addrs()?
        .next()
        .ok_or_else(|| io::Error::from(io::ErrorKind::AddrNotAvailable))?;
    let certs = load_certs(Path::new("cert.pem"))?;
    let mut keys = load_keys(Path::new("private_key.pem"))?;

    let mut config = rustls::ServerConfig::builder()
        .with_safe_defaults()
        .with_no_client_auth()
        .with_single_cert(certs, keys.remove(0))
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidInput, err))?;

    // Allow http2 for ALPN negociation with the client
    config.alpn_protocols = vec![b"h2".to_vec()];

    let acceptor = TlsAcceptor::from(Arc::new(config));
    let listener_socket = TcpListener::bind(&addr).await?;

    loop {
        let (client_socket, _) = listener_socket.accept().await?;
        let acceptor = acceptor.clone();

        tokio::spawn(async move {
            let fut = async move { do_connection(acceptor, client_socket).await };
            if let Err(err) = fut.await {
                eprintln!("future error: {:?}", err);
            }
        });
    }
}
