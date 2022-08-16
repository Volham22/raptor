use std::{collections::HashMap, net::SocketAddr, str::FromStr};

use tokio::net::TcpListener;

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
            let listener = TcpListener::bind(&addr).await?;
            println!("Listenning on {} for {}", addr_str, vhost.name);

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
                        let mut conn = Connection::new(stream, &vhost.root_dir);
                        conn.read_request().await;
                    });
                }
            });
        }

        Ok(())
    }
}
