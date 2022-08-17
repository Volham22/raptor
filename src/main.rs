use std::process;

use clap::Parser;
use vhosts::ListenerError;

use crate::{
    config::{error::ConfigError, Config},
    vhosts::VhostManager,
};

mod config;
mod connection;
mod handlers;
mod stream_utils;
mod vhosts;

#[derive(Parser)]
#[clap(author, version, about, long_about = None)]
/// Raptor web server. Fast and light.
struct Cli {
    /// Config file for the server in json
    config_file: String,
}

#[tokio::main]
pub async fn main() -> ListenerError {
    let cli = Cli::parse();
    let config: Config = match Config::from_file::<ConfigError>(&cli.config_file).await {
        Ok(c) => c,
        Err(msg) => {
            eprintln!("{}", msg);
            process::exit(1);
        }
    };

    println!("{:?}", config);
    let manager = Box::new(VhostManager::from_config(config));

    // We can leak the vhost manager since, this the manager has to live during
    // the whole program runtime. Only one vhost manager is instantiated.
    Box::leak(manager).init_listeners().await?;

    loop {}
}
