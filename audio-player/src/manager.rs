use std::process::Stdio;
use thiserror::Error;
use tokio::process::{Child, Command};

#[derive(Debug, Error)]
pub enum ManagerError {
    #[error("process {0} is already running")]
    AlreadyRunning(&'static str),
    #[error("failed to start {0}: {1}")]
    SpawnFailed(&'static str, std::io::Error),
    #[error("failed to kill {0}: {1}")]
    KillFailed(&'static str, std::io::Error),
    #[error("failed to wait for {0}: {1}")]
    WaitFailed(&'static str, std::io::Error),
    #[error("{0} has no PID")]
    NoPid(&'static str),
    #[error("failed to signal {0}: {1}")]
    SignalFailed(&'static str, nix::errno::Errno),
}

pub struct Manager {
    name: &'static str,
    bin_path: &'static str,
    args: Vec<String>,
    child: Option<Child>,
}

impl Manager {
    pub fn new(name: &'static str, bin: &'static str, args: Vec<String>) -> Self {
        Manager {
            name,
            bin_path: bin,
            args,
            child: None,
        }
    }

    pub async fn start(&mut self) -> Result<(), ManagerError> {
        if self.child.is_some() {
            return Err(ManagerError::AlreadyRunning(self.name));
        }
        let child = Command::new(self.bin_path)
            .args(&self.args)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| ManagerError::SpawnFailed(self.name, e))?;
        self.child = Some(child);
        Ok(())
    }

    pub async fn stop(&mut self) -> Result<(), ManagerError> {
        if let Some(mut child) = self.child.take() {
            child
                .kill()
                .await
                .map_err(|e| ManagerError::KillFailed(self.name, e))?;
            child
                .wait()
                .await
                .map_err(|e| ManagerError::WaitFailed(self.name, e))?;
        }
        Ok(())
    }

    pub async fn pause(&mut self) -> Result<(), ManagerError> {
        if let Some(child) = &self.child {
            let pid = child.id().ok_or(ManagerError::NoPid(self.name))?;
            nix::sys::signal::kill(
                nix::unistd::Pid::from_raw(pid as i32),
                nix::sys::signal::Signal::SIGSTOP,
            )
            .map_err(|e| ManagerError::SignalFailed(self.name, e))?;
        }
        Ok(())
    }

    pub async fn resume(&mut self) -> Result<(), ManagerError> {
        if let Some(child) = &self.child {
            let pid = child.id().ok_or(ManagerError::NoPid(self.name))?;
            nix::sys::signal::kill(
                nix::unistd::Pid::from_raw(pid as i32),
                nix::sys::signal::Signal::SIGCONT,
            )
            .map_err(|e| ManagerError::SignalFailed(self.name, e))?;
        }
        Ok(())
    }
}
