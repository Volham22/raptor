use std::{io, net::IpAddr, path, str::FromStr};

use clap::Parser;
use serde::Deserialize;
use thiserror::Error;
use tokio::fs;
use tracing::info;

const DEFAULT_VALUE_DEFAULT_FILE: &str = "index.html";

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
    #[error("Default file is absolute: '{0}'")]
    AbsoluteDefaultFile(path::PathBuf),
}

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub ip: IpAddr,
    pub port: u16,
    pub cert_path: path::PathBuf,
    pub key_path: path::PathBuf,
    pub root_dir: path::PathBuf,
    pub default_file: Option<path::PathBuf>,
    pub log_file: Option<path::PathBuf>,
}

pub async fn read_config_from_file(path: &path::Path) -> Result<String, ConfigError> {
    fs::read(path)
        .await
        .map(|s| String::from_utf8_lossy(&s).to_string())
        .map_err(ConfigError::ReadError)
}

impl Config {
    pub async fn from_yaml_str(config: &str) -> Result<Config, ConfigError> {
        let result: Config = serde_yaml::from_str(config).map_err(ConfigError::BadYaml)?;

        if result.root_dir.is_file() {
            return Err(ConfigError::RootFolderIsAFile(result.root_dir));
        }

        if result.root_dir.is_relative() {
            return Err(ConfigError::RootPathNotAbsolute(result.root_dir));
        }

        if let Some(log_file) = result.log_file.as_ref() {
            info!("Logging at: {}", log_file.to_str().unwrap());
        }

        if let Some(default_file) = result.default_file.as_ref() {
            if default_file.is_absolute() {
                return Err(ConfigError::AbsoluteDefaultFile(default_file.clone()));
            }
        }

        Ok(result)
    }

    pub fn get_default_file(&self) -> path::PathBuf {
        self.default_file
            .clone()
            .unwrap_or(path::PathBuf::from_str(DEFAULT_VALUE_DEFAULT_FILE).expect("unreachable"))
    }
}

#[cfg(test)]
mod tests {
    use crate::config::{Config, ConfigError};

    #[tokio::test]
    async fn test_bad_yaml() {
        let err = Config::from_yaml_str("this is not yaml").await;
        match err {
            Err(ConfigError::BadYaml(_)) => (),
            _ => panic!("should be bad yaml: {err:?}"),
        }
    }

    #[tokio::test]
    async fn test_root_dir_is_file() {
        let err = Config::from_yaml_str(
            r#"
ip: "127.0.0.1"
port: 8000
cert_path: "./localhost.pem"
key_path: "./localhost-key.pem"
root_dir: /proc/cpuinfo
            "#,
        )
        .await;
        match err {
            Err(ConfigError::RootFolderIsAFile(_)) => (),
            _ => panic!("wrong error: {err:?}"),
        }
    }

    #[tokio::test]
    async fn test_root_path_is_not_absolute() {
        let err = Config::from_yaml_str(
            r#"
ip: "127.0.0.1"
port: 8000
cert_path: "./localhost.pem"
key_path: "./localhost-key.pem"
root_dir: ./path/to/dir
            "#,
        )
        .await;
        match err {
            Err(ConfigError::RootPathNotAbsolute(_)) => (),
            _ => panic!("wrong error: {err:?}"),
        }
    }

    #[tokio::test]
    async fn test_default_file_is_absolute() {
        let err = Config::from_yaml_str(
            r#"
ip: "127.0.0.1"
port: 8000
cert_path: "./localhost.pem"
key_path: "./localhost-key.pem"
root_dir: /tmp
default_file: /tmp/default.txt
            "#,
        )
        .await;
        match err {
            Err(ConfigError::AbsoluteDefaultFile(_)) => (),
            _ => panic!("wrong error: {err:?}"),
        }
    }
}
