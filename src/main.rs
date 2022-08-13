use std::{
    net::SocketAddr,
    process::{self, ExitCode},
};

use tokio::net::TcpListener;

use crate::{
    config::{error::ConfigError, Config},
    connection::Connection,
};

mod config;
mod connection;
mod handlers;
mod stream_utils;

#[tokio::main]
pub async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let config: Config = match Config::from_file::<ConfigError>("config.json").await {
        Ok(c) => c,
        Err(msg) => {
            eprintln!("{}", msg);
            process::exit(1);
        }
    };

    println!("{:?}", config);

    let addr: SocketAddr = ([127, 0, 0, 1], 3000).into();

    let listener = TcpListener::bind(addr).await?;
    println!("Listening on http://{}", addr);
    loop {
        let (stream, _) = listener.accept().await?;

        tokio::task::spawn(async move {
            let mut conn = Connection::new(stream);
            conn.read_request().await;
        });
    }
}
