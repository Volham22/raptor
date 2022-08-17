use std::{
    collections::HashMap,
    fs::File,
    io::{self, BufReader},
    net::SocketAddr,
    path::Path,
    str::FromStr,
    sync::Arc,
};

use rustls_pemfile::{certs, rsa_private_keys};
use tokio::net::TcpListener;
use tokio_rustls::{
    rustls::{self, Certificate, PrivateKey},
    TlsAcceptor,
};

use crate::{
    config::{Config, Vhost},
    connection::Connection,
};

pub struct VhostManager {
    vhosts: HashMap<String, Vhost>,
}

pub type ListenerError = Result<(), Box<dyn std::error::Error + Send + Sync>>;

impl VhostManager {
    pub fn from_config(config: Config) -> Self {
        let mut map = HashMap::new();

        for vhost in config.vhosts {
            map.insert(vhost.name.to_string(), vhost);
        }

        Self { vhosts: map }
    }

    pub async fn init_listeners(&'static self) -> ListenerError {
        for vhost in self.vhosts.values() {
            let addr_str = if vhost.is_ipv6 {
                format!("[{}]:{}", vhost.ip, vhost.port)
            } else {
                format!("{}:{}", vhost.ip, vhost.port)
            };

            let addr = SocketAddr::from_str(&addr_str).unwrap();
            if vhost.is_tls() {
                Self::init_tls_listener(addr, vhost).await?;
            } else {
                Self::init_listener(addr, &vhost.root_dir).await?;
            }
            println!("Listenning on {} for {}", addr_str, &vhost.name);
        }

        Ok(())
    }

    async fn init_listener(addr: SocketAddr, root_dir: &'static str) -> ListenerError {
        let listener = TcpListener::bind(&addr).await?;

        tokio::spawn(async move {
            loop {
                let (stream, _) = match listener.accept().await {
                    Ok(res) => res,
                    Err(msg) => {
                        eprintln!("Error accepting socket!: '{}'", msg.to_string());
                        return;
                    }
                };

                tokio::spawn(async move {
                    let mut conn = Connection::new(stream, root_dir);
                    conn.read_request().await;
                });
            }
        });

        Ok(())
    }

    async fn init_tls_listener(addr: SocketAddr, vhost: &'static Vhost) -> ListenerError {
        let certs = load_certs(&Path::new(&vhost.cert_key.as_ref().unwrap()))?;
        let mut keys = load_keys(&Path::new(&vhost.private_key.as_ref().unwrap()))?;
        let config = rustls::ServerConfig::builder()
            .with_safe_defaults()
            .with_no_client_auth()
            .with_single_cert(certs, keys.remove(0))?;

        let tls_acceptor = TlsAcceptor::from(Arc::new(config));
        let listener = TcpListener::bind(&addr).await?;

        tokio::spawn(async move {
            loop {
                let stream = match listener.accept().await {
                    Ok(res) => match tls_acceptor.accept(res.0).await {
                        Ok(res) => res,
                        Err(msg) => {
                            eprintln!("Error accepting tls socket: '{}'", msg.to_string());
                            continue;
                        }
                    },
                    Err(msg) => {
                        eprintln!("Error accepting socket!: '{}'", msg.to_string());
                        continue;
                    }
                };

                tokio::spawn(async move {
                    let mut conn = Connection::new(stream, &vhost.root_dir);
                    conn.read_request().await;
                });
            }
        });
        Ok(())
    }
}

// From rusttls example
fn load_certs(path: &Path) -> io::Result<Vec<Certificate>> {
    certs(&mut BufReader::new(File::open(path)?))
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid cert"))
        .map(|mut certs| certs.drain(..).map(Certificate).collect())
}

// From rusttls example
fn load_keys(path: &Path) -> io::Result<Vec<PrivateKey>> {
    rsa_private_keys(&mut BufReader::new(File::open(path)?))
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid key"))
        .map(|mut keys| keys.drain(..).map(PrivateKey).collect())
}
