use std::{collections::BTreeMap, fs, net::SocketAddr, path::PathBuf};

use anyhow::{Context, Result, bail};
use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub server: ServerConfig,
    #[serde(default)]
    pub limits: LimitConfig,
    #[serde(default)]
    pub targets: Vec<TargetConfig>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ServerConfig {
    #[serde(default = "default_bind")]
    pub bind: SocketAddr,
    #[serde(default = "default_name")]
    pub name: String,
    #[serde(default = "default_instructions")]
    pub instructions: String,
    #[serde(default)]
    pub allowed_origins: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct LimitConfig {
    #[serde(default = "default_max_command_chars")]
    pub max_command_chars: usize,
    #[serde(default = "default_max_output_bytes")]
    pub max_output_bytes: usize,
    #[serde(default = "default_max_timeout_seconds")]
    pub max_timeout_seconds: u64,
    #[serde(default = "default_timeout_seconds")]
    pub default_timeout_seconds: u64,
}

#[derive(Clone, Debug, Deserialize)]
pub struct TargetConfig {
    pub name: String,
    pub host: String,
    #[serde(default = "default_ssh_port")]
    pub port: u16,
    pub user: String,
    pub key_path: PathBuf,
    pub known_hosts_path: PathBuf,
    #[serde(default = "default_connect_timeout_seconds")]
    pub connect_timeout_seconds: u64,
    #[serde(default = "default_server_alive_interval_seconds")]
    pub server_alive_interval_seconds: u64,
    #[serde(default = "default_server_alive_count_max")]
    pub server_alive_count_max: u64,
}

impl Config {
    pub fn load(path: PathBuf) -> Result<Self> {
        let text = fs::read_to_string(&path)
            .with_context(|| format!("failed to read config {}", path.display()))?;
        let config: Self = toml::from_str(&text)
            .with_context(|| format!("failed to parse config {}", path.display()))?;
        config.validate()?;
        Ok(config)
    }

    pub fn targets_by_name(&self) -> BTreeMap<String, TargetConfig> {
        self.targets
            .iter()
            .cloned()
            .map(|target| (target.name.clone(), target))
            .collect()
    }

    fn validate(&self) -> Result<()> {
        if self.targets.is_empty() {
            bail!("at least one target must be configured");
        }
        let mut seen = BTreeMap::<&str, ()>::new();
        for target in &self.targets {
            if target.name.trim().is_empty() {
                bail!("target name must not be empty");
            }
            if target.name.contains('/') || target.name.contains('\\') {
                bail!("target name must not contain slashes: {}", target.name);
            }
            if seen.insert(target.name.as_str(), ()).is_some() {
                bail!("duplicate target name: {}", target.name);
            }
            if target.host.trim().is_empty() {
                bail!("target host must not be empty: {}", target.name);
            }
            if target.user.trim().is_empty() {
                bail!("target user must not be empty: {}", target.name);
            }
            if target.key_path.as_os_str().is_empty() {
                bail!("target key_path must not be empty: {}", target.name);
            }
            if target.known_hosts_path.as_os_str().is_empty() {
                bail!("target known_hosts_path must not be empty: {}", target.name);
            }
        }
        if self.limits.default_timeout_seconds == 0 {
            bail!("default_timeout_seconds must be positive");
        }
        if self.limits.default_timeout_seconds > self.limits.max_timeout_seconds {
            bail!("default_timeout_seconds must be <= max_timeout_seconds");
        }
        Ok(())
    }
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind: default_bind(),
            name: default_name(),
            instructions: default_instructions(),
            allowed_origins: Vec::new(),
        }
    }
}

impl Default for LimitConfig {
    fn default() -> Self {
        Self {
            max_command_chars: default_max_command_chars(),
            max_output_bytes: default_max_output_bytes(),
            max_timeout_seconds: default_max_timeout_seconds(),
            default_timeout_seconds: default_timeout_seconds(),
        }
    }
}

fn default_bind() -> SocketAddr {
    "0.0.0.0:8000".parse().expect("valid default bind")
}

fn default_name() -> String {
    "SSH MCP".to_string()
}

fn default_instructions() -> String {
    "Run commands on explicitly configured SSH targets. Use ssh_targets before ssh_run when target availability is uncertain.".to_string()
}

fn default_max_command_chars() -> usize {
    16_384
}

fn default_max_output_bytes() -> usize {
    65_536
}

fn default_max_timeout_seconds() -> u64 {
    300
}

fn default_timeout_seconds() -> u64 {
    60
}

fn default_ssh_port() -> u16 {
    22
}

fn default_connect_timeout_seconds() -> u64 {
    10
}

fn default_server_alive_interval_seconds() -> u64 {
    10
}

fn default_server_alive_count_max() -> u64 {
    2
}
