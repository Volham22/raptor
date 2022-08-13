use serde::Deserialize;
use tokio::fs;

#[derive(Deserialize, Debug)]
pub struct Config {
    vhosts: Vec<Vhost>,
}

impl Config {
    pub async fn from_file<E>(path: &str) -> Result<Config, E>
    where
        E: From<std::io::Error> + From<serde_json::Error>,
    {
        let content = fs::read_to_string(path).await?;
        Ok(serde_json::from_str(content.as_str())?)
    }
}

#[derive(Deserialize, Debug)]
pub struct Vhost {
    ip: String,
    port: u16,
    is_ipv6: bool,
}

pub mod error {
    use std::{
        fmt::{Debug, Display},
        io,
    };

    #[derive(Debug)]
    pub enum ConfigError {
        IO(io::Error),
        Serde(serde_json::Error),
    }

    impl From<std::io::Error> for ConfigError {
        fn from(e: std::io::Error) -> Self {
            ConfigError::IO(e)
        }
    }

    impl From<serde_json::Error> for ConfigError {
        fn from(e: serde_json::Error) -> Self {
            ConfigError::Serde(e)
        }
    }

    impl Display for ConfigError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                ConfigError::IO(e) => f.write_fmt(format_args!("{}", e)),
                ConfigError::Serde(e) => f.write_fmt(format_args!("{}", e)),
            }
        }
    }
}
