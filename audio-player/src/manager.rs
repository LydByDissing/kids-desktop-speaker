use std::process::{ExitStatus, Stdio};
use std::time::{Duration, Instant};
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

/// How long to wait before restarting a source that exited (AUD_REQ_002).
/// The wait doubles after each quick failure and resets once the source has
/// stayed up for `stable_after`.
#[derive(Debug, Clone, Copy)]
pub struct Backoff {
    pub min: Duration,
    pub max: Duration,
    pub stable_after: Duration,
}

impl Default for Backoff {
    fn default() -> Self {
        Backoff {
            min: Duration::from_secs(1),
            max: Duration::from_secs(60),
            stable_after: Duration::from_secs(60),
        }
    }
}

/// What a call to [`Manager::supervise`] did.
#[derive(Debug, PartialEq, Eq)]
pub enum Supervision {
    /// Nothing to do: running, stopped on purpose, or waiting to restart.
    Idle,
    /// The source exited and will be restarted after the given delay.
    Exited {
        status: String,
        restart_in: Duration,
    },
    /// The source was restarted.
    Restarted,
    /// A restart was attempted and failed; it will be tried again after the delay.
    RestartFailed { error: String, retry_in: Duration },
}

pub struct Manager {
    name: &'static str,
    bin_path: &'static str,
    args: Vec<String>,
    child: Option<Child>,
    backoff: Backoff,
    /// Set once the source has started; a source that never started is not supervised.
    supervised: bool,
    started_at: Option<Instant>,
    delay: Duration,
    restart_at: Option<Instant>,
}

impl Manager {
    pub fn new(name: &'static str, bin: &'static str, args: Vec<String>) -> Self {
        Self::with_backoff(name, bin, args, Backoff::default())
    }

    pub fn with_backoff(
        name: &'static str,
        bin: &'static str,
        args: Vec<String>,
        backoff: Backoff,
    ) -> Self {
        Manager {
            name,
            bin_path: bin,
            args,
            child: None,
            backoff,
            supervised: false,
            started_at: None,
            delay: backoff.min,
            restart_at: None,
        }
    }

    pub fn name(&self) -> &'static str {
        self.name
    }

    pub fn is_running(&self) -> bool {
        self.child.is_some()
    }

    pub fn pid(&self) -> Option<u32> {
        self.child.as_ref().and_then(|c| c.id())
    }

    pub async fn start(&mut self) -> Result<(), ManagerError> {
        if self.child.is_some() {
            return Err(ManagerError::AlreadyRunning(self.name));
        }
        // stdout/stderr are inherited so the source's own messages reach the journal.
        let child = Command::new(self.bin_path)
            .args(&self.args)
            .stdin(Stdio::null())
            .spawn()
            .map_err(|e| ManagerError::SpawnFailed(self.name, e))?;
        self.child = Some(child);
        self.supervised = true;
        self.started_at = Some(Instant::now());
        self.restart_at = None;
        Ok(())
    }

    /// Starts the source if it can. A missing source is reported rather than fatal,
    /// so the rest of the speaker keeps working without it. Returns whether it started.
    pub async fn start_if_available(&mut self) -> bool {
        match self.start().await {
            Ok(()) => true,
            Err(e) => {
                eprintln!("audio-player: source unavailable: {e}");
                false
            }
        }
    }

    /// Reaps the source if it has exited, and restarts it once its backoff has
    /// passed. Call it periodically. Does nothing for a source that never
    /// started or was stopped on purpose.
    pub async fn supervise(&mut self) -> Supervision {
        if !self.supervised {
            return Supervision::Idle;
        }
        if let Some(child) = self.child.as_mut() {
            let status = match child.try_wait() {
                Ok(Some(status)) => describe(status),
                Ok(None) => return Supervision::Idle,
                Err(e) => format!("unknown status ({e})"),
            };
            self.child = None;
            let ran_for = self.started_at.map(|t| t.elapsed()).unwrap_or_default();
            if ran_for >= self.backoff.stable_after {
                self.delay = self.backoff.min;
            }
            let restart_in = self.delay;
            self.schedule_restart();
            return Supervision::Exited { status, restart_in };
        }
        match self.restart_at {
            Some(at) if Instant::now() >= at => match self.start().await {
                Ok(()) => Supervision::Restarted,
                Err(e) => {
                    let retry_in = self.delay;
                    self.schedule_restart();
                    Supervision::RestartFailed {
                        error: e.to_string(),
                        retry_in,
                    }
                }
            },
            _ => Supervision::Idle,
        }
    }

    /// Schedules a restart after the current delay and doubles the delay for next time.
    fn schedule_restart(&mut self) {
        self.restart_at = Some(Instant::now() + self.delay);
        self.delay = (self.delay * 2).min(self.backoff.max);
    }

    pub async fn stop(&mut self) -> Result<(), ManagerError> {
        self.supervised = false;
        self.restart_at = None;
        if let Some(mut child) = self.child.take() {
            // The child may already have exited; kill() on a reaped child is fine.
            if child.try_wait().ok().flatten().is_none() {
                child
                    .kill()
                    .await
                    .map_err(|e| ManagerError::KillFailed(self.name, e))?;
            }
            child
                .wait()
                .await
                .map_err(|e| ManagerError::WaitFailed(self.name, e))?;
        }
        Ok(())
    }

    pub async fn pause(&mut self) -> Result<(), ManagerError> {
        self.signal(nix::sys::signal::Signal::SIGSTOP)
    }

    pub async fn resume(&mut self) -> Result<(), ManagerError> {
        self.signal(nix::sys::signal::Signal::SIGCONT)
    }

    fn signal(&self, sig: nix::sys::signal::Signal) -> Result<(), ManagerError> {
        if let Some(child) = &self.child {
            let pid = child.id().ok_or(ManagerError::NoPid(self.name))?;
            nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid as i32), sig)
                .map_err(|e| ManagerError::SignalFailed(self.name, e))?;
        }
        Ok(())
    }
}

fn describe(status: ExitStatus) -> String {
    use std::os::unix::process::ExitStatusExt;
    match (status.code(), status.signal()) {
        (Some(code), _) => format!("exit code {code}"),
        (None, Some(sig)) => format!("signal {sig}"),
        _ => status.to_string(),
    }
}
