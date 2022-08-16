use std::process;

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

#[tokio::main]
pub async fn main() -> ListenerError {
    let config: Config = match Config::from_file::<ConfigError>("config.json").await {
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
