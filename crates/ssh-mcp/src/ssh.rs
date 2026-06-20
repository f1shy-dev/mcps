use std::{collections::BTreeMap, process::Stdio, time::Duration};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::{io::AsyncReadExt, process::Command, time};

use crate::config::{LimitConfig, TargetConfig};

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SshRunInput {
    pub target: String,
    pub command: String,
    #[serde(default)]
    pub timeout_seconds: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct SshRunOutput {
    pub target: String,
    pub exit_code: Option<i32>,
    pub timed_out: bool,
    pub stdout: String,
    pub stderr: String,
    pub stdout_truncated: bool,
    pub stderr_truncated: bool,
}

#[derive(Debug, Error)]
pub enum SshRunError {
    #[error("unknown target: {0}")]
    UnknownTarget(String),
    #[error("command must contain 1-{0} characters")]
    InvalidCommandLength(usize),
    #[error("timeout_seconds must be between 1 and {0}")]
    InvalidTimeout(u64),
    #[error("failed to spawn ssh: {0}")]
    Spawn(std::io::Error),
    #[error("failed to wait for ssh output: {0}")]
    Wait(std::io::Error),
}

impl SshRunError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::UnknownTarget(_) => "unknown_target",
            Self::InvalidCommandLength(_) => "invalid_command",
            Self::InvalidTimeout(_) => "invalid_timeout",
            Self::Spawn(_) => "spawn_error",
            Self::Wait(_) => "wait_error",
        }
    }
}

pub async fn run_ssh_command(
    limits: &LimitConfig,
    targets: &BTreeMap<String, TargetConfig>,
    input: SshRunInput,
) -> Result<SshRunOutput, SshRunError> {
    let target = targets
        .get(&input.target)
        .ok_or_else(|| SshRunError::UnknownTarget(input.target.clone()))?;
    if input.command.is_empty() || input.command.len() > limits.max_command_chars {
        return Err(SshRunError::InvalidCommandLength(limits.max_command_chars));
    }
    let timeout_seconds = input
        .timeout_seconds
        .unwrap_or(limits.default_timeout_seconds);
    if timeout_seconds == 0 || timeout_seconds > limits.max_timeout_seconds {
        return Err(SshRunError::InvalidTimeout(limits.max_timeout_seconds));
    }

    let destination = format!("{}@{}", target.user, target.host);
    let remote_command = format!("bash -lc {}", shell_words::quote(&input.command));
    let mut child = Command::new("/usr/bin/ssh")
        .arg("-o")
        .arg("BatchMode=yes")
        .arg("-o")
        .arg("IdentitiesOnly=yes")
        .arg("-o")
        .arg(format!("IdentityFile={}", target.key_path.display()))
        .arg("-o")
        .arg(format!(
            "UserKnownHostsFile={}",
            target.known_hosts_path.display()
        ))
        .arg("-o")
        .arg("StrictHostKeyChecking=yes")
        .arg("-o")
        .arg("PasswordAuthentication=no")
        .arg("-o")
        .arg("KbdInteractiveAuthentication=no")
        .arg("-o")
        .arg("ForwardAgent=no")
        .arg("-o")
        .arg("ClearAllForwardings=yes")
        .arg("-o")
        .arg("RequestTTY=no")
        .arg("-o")
        .arg(format!("ConnectTimeout={}", target.connect_timeout_seconds))
        .arg("-o")
        .arg(format!(
            "ServerAliveInterval={}",
            target.server_alive_interval_seconds
        ))
        .arg("-o")
        .arg(format!(
            "ServerAliveCountMax={}",
            target.server_alive_count_max
        ))
        .arg("-p")
        .arg(target.port.to_string())
        .arg(destination)
        .arg(remote_command)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(SshRunError::Spawn)?;

    let mut stdout_pipe = child.stdout.take().expect("stdout is piped");
    let mut stderr_pipe = child.stderr.take().expect("stderr is piped");
    let stdout_task = tokio::spawn(async move {
        let mut data = Vec::new();
        stdout_pipe.read_to_end(&mut data).await.map(|_| data)
    });
    let stderr_task = tokio::spawn(async move {
        let mut data = Vec::new();
        stderr_pipe.read_to_end(&mut data).await.map(|_| data)
    });

    let mut timed_out = false;
    let status = match time::timeout(Duration::from_secs(timeout_seconds), child.wait()).await {
        Ok(result) => result.map_err(SshRunError::Wait)?,
        Err(_) => {
            timed_out = true;
            let _ = child.kill().await;
            child.wait().await.map_err(SshRunError::Wait)?
        }
    };

    let stdout_data = stdout_task
        .await
        .map_err(|error| SshRunError::Wait(std::io::Error::other(error)))?
        .map_err(SshRunError::Wait)?;
    let stderr_data = stderr_task
        .await
        .map_err(|error| SshRunError::Wait(std::io::Error::other(error)))?
        .map_err(SshRunError::Wait)?;

    let (stdout, stdout_truncated) = trim_output(&stdout_data, limits.max_output_bytes);
    let (stderr, stderr_truncated) = trim_output(&stderr_data, limits.max_output_bytes);

    Ok(SshRunOutput {
        target: input.target,
        exit_code: if timed_out { None } else { status.code() },
        timed_out,
        stdout,
        stderr,
        stdout_truncated,
        stderr_truncated,
    })
}

fn trim_output(data: &[u8], max_bytes: usize) -> (String, bool) {
    let truncated = data.len() > max_bytes;
    let end = if truncated { max_bytes } else { data.len() };
    (
        String::from_utf8_lossy(&data[..end]).into_owned(),
        truncated,
    )
}
