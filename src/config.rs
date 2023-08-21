use std::{io, net::IpAddr, path};

use clap::Parser;
use serde::Deserialize;
use thiserror::Error;
use tokio::fs;

#[derive(Parser)]
#[command(name = "raptor")]
#[command(version = "0.1")]
#[command(about = "HTTP server", long_about = Some("A lightweight and easy to use HTTP server"))]
pub struct CliConfig {
    #[arg(short, long)]
    pub config: path::PathBuf,
    #[arg(
        short,
        long,
        help = "Logging level. Can an integer between 1-5 or error, warn, info, debug and trace",
        default_value_t = tracing::Level::INFO
    )]
    pub level: tracing::Level,
}

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Bad Yaml: {0:?}")]
    BadYaml(serde_yaml::Error),
    #[error("Error while reading config file: '{0}'")]
    ReadError(io::Error),
    #[error("Root path is a file: '{0}'")]
    RootFolderIsAFile(path::PathBuf),
    #[error("Root path is not absolute: '{0}'")]
    RootPathNotAbsolute(path::PathBuf),
}

#[derive(Clone, Deserialize)]
pub struct Config {
    pub ip: IpAddr,
    pub port: u16,
    pub cert_path: path::PathBuf,
    pub key_path: path::PathBuf,
    pub root_dir: path::PathBuf,
}

pub async fn read_config_from_file(path: &path::Path) -> Result<String, ConfigError> {
    fs::read(path)
        .await
        .map(|s| String::from_utf8_lossy(&s).to_string())
        .map_err(ConfigError::ReadError)
}

impl Config {
    pub fn from_yaml_str(config: &str) -> Result<Config, ConfigError> {
        let result: Config = serde_yaml::from_str(config).map_err(ConfigError::BadYaml)?;

        if result.root_dir.is_file() {
            return Err(ConfigError::RootFolderIsAFile(result.root_dir));
        }

        if result.root_dir.is_relative() {
            return Err(ConfigError::RootPathNotAbsolute(result.root_dir));
        }

        Ok(result)
    }
}
