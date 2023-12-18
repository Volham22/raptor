use std::{fs::File, io, net::SocketAddr, path::Path, sync::Arc};

use clap::Parser;
use connection::do_connection;
use rustls_pemfile::{read_all, rsa_private_keys};
use tokio::net::TcpListener;
use tokio_rustls::{
    rustls::{self, Certificate, PrivateKey},
    TlsAcceptor,
};
use tracing::{debug, error, info, warn};

mod config;
mod connection;
mod http11;
mod http2;
mod method_handlers;
mod request;
mod response;

fn load_certs(path: &Path) -> io::Result<Vec<Certificate>> {
    let mut cert_buffer = io::BufReader::new(File::open(path)?);

    Ok(read_all(&mut cert_buffer)?
        .into_iter()
        .map(|c| match c {
            rustls_pemfile::Item::X509Certificate(bytes) => Certificate(bytes),
            _ => {
                error!(
                    "Invalid certificate found in pem file: {}",
                    path.to_str().unwrap()
                );
                std::process::exit(1);
            }
        })
        .collect())
}

fn load_keys(path: &Path) -> io::Result<Vec<PrivateKey>> {
    let mut key_buffer = io::BufReader::new(File::open(path)?);

    // TODO: Allow all type of private key
    rsa_private_keys(&mut key_buffer)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid key file"))
        .map(|mut keys| keys.drain(..).map(PrivateKey).collect())
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let cli_conf = config::CliConfig::parse();
    tracing_subscriber::fmt()
        .with_max_level(cli_conf.level)
        .init();

    let config_file_content = match config::read_config_from_file(&cli_conf.config).await {
        Ok(c) => c,
        Err(e) => {
            error!("{}", e);
            std::process::exit(1);
        }
    };

    let conf = match config::Config::from_yaml_str(&config_file_content) {
        Ok(c) => Arc::new(c),
        Err(err) => {
            error!("Error while loading configuration file: {}", err);
            std::process::exit(1);
        }
    };

    info!("Begin of server");

    let addr = SocketAddr::new(conf.ip, conf.port);
    debug!("Created sockaddr");
    let certs = load_certs(&conf.cert_path)?;
    let mut keys = load_keys(&conf.key_path)?;
    debug!("Loaded certs and private key");

    let mut config = rustls::ServerConfig::builder()
        .with_safe_defaults()
        .with_no_client_auth()
        .with_single_cert(certs, keys.remove(0))
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidInput, err))?;

    // Allow http2 for ALPN negociation with the client
    config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
    let shared_config = Arc::new(config);
    debug!(
        "Supported ALPN protocols: {:?}",
        shared_config.alpn_protocols
    );

    let acceptor = TlsAcceptor::from(shared_config.clone());
    let listener_socket = TcpListener::bind(&addr).await?;
    info!("Start to listen on {:?}", &addr);
    info!("Serving from root directory: {:?}", conf.root_dir);

    loop {
        let (client_socket, client_addr) = listener_socket.accept().await?;
        let acceptor = acceptor.clone();
        info!("New client connection from {client_addr:?}");
        let thread_conf = conf.clone();

        tokio::spawn(async move {
            info!("Started connection handling for {client_addr:?}");
            let fut = async move {
                // client_socket.set_nodelay(true)?;
                do_connection(acceptor, client_socket, thread_conf).await
            };

            if let Err(err) = fut.await {
                warn!("client connection handler error: {:?}", err);
            }
        });
    }
}
