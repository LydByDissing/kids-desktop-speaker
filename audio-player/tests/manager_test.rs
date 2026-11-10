use audio_player::manager::Manager;

#[tokio::test]
async fn missing_source_is_not_fatal() {
    let mut manager = Manager::new("missing", "/nonexistent/source", vec![]);
    assert!(!manager.start_if_available().await);
    assert!(manager.stop().await.is_ok());
}

#[tokio::test]
async fn available_source_starts() {
    let mut manager = Manager::new("sleep", "/bin/sleep", vec!["30".to_string()]);
    assert!(manager.start_if_available().await);
    assert!(manager.stop().await.is_ok());
}

use audio_player::manager::{Backoff, Supervision};
use std::time::Duration;

fn quick() -> Backoff {
    Backoff {
        min: Duration::from_millis(50),
        max: Duration::from_millis(200),
        stable_after: Duration::from_secs(60),
    }
}

/// Polls supervise() until it reports something other than Idle.
async fn next_event(manager: &mut Manager) -> Supervision {
    for _ in 0..200 {
        let event = manager.supervise().await;
        if event != Supervision::Idle {
            return event;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("no supervision event within 2 s");
}

fn is_zombie(pid: u32) -> bool {
    std::fs::read_to_string(format!("/proc/{pid}/stat"))
        .map(|s| {
            s.rsplit(')')
                .next()
                .unwrap_or("")
                .trim_start()
                .starts_with('Z')
        })
        .unwrap_or(false)
}

#[tokio::test]
async fn exited_source_is_reaped_and_restarted() {
    let mut manager = Manager::with_backoff("true", "/bin/true", vec![], quick());
    assert!(manager.start_if_available().await);
    let first = manager.pid().unwrap();

    match next_event(&mut manager).await {
        Supervision::Exited { status, restart_in } => {
            assert_eq!(status, "exit code 0");
            assert_eq!(restart_in, Duration::from_millis(50));
        }
        other => panic!("expected Exited, got {other:?}"),
    }
    assert!(!manager.is_running());
    assert!(!is_zombie(first), "exited source was left as a zombie");

    assert_eq!(next_event(&mut manager).await, Supervision::Restarted);
    assert!(manager.is_running());
    assert_ne!(manager.pid(), Some(first));
    manager.stop().await.unwrap();
}

#[tokio::test]
async fn backoff_doubles_up_to_the_maximum() {
    let mut manager = Manager::with_backoff("false", "/bin/false", vec![], quick());
    assert!(manager.start_if_available().await);
    let mut delays = Vec::new();
    while delays.len() < 4 {
        if let Supervision::Exited { status, restart_in } = next_event(&mut manager).await {
            assert_eq!(status, "exit code 1");
            delays.push(restart_in.as_millis());
        }
    }
    assert_eq!(delays, vec![50, 100, 200, 200]);
    manager.stop().await.unwrap();
}

#[tokio::test]
async fn killed_source_reports_the_signal() {
    let mut manager = Manager::with_backoff("sleep", "/bin/sleep", vec!["30".to_string()], quick());
    assert!(manager.start_if_available().await);
    let pid = manager.pid().unwrap() as i32;
    nix::sys::signal::kill(
        nix::unistd::Pid::from_raw(pid),
        nix::sys::signal::Signal::SIGKILL,
    )
    .unwrap();
    match next_event(&mut manager).await {
        Supervision::Exited { status, .. } => assert_eq!(status, "signal 9"),
        other => panic!("expected Exited, got {other:?}"),
    }
    manager.stop().await.unwrap();
}

#[tokio::test]
async fn stopped_or_missing_source_is_not_restarted() {
    let mut missing = Manager::with_backoff("missing", "/nonexistent/source", vec![], quick());
    assert!(!missing.start_if_available().await);
    let mut stopped = Manager::with_backoff("sleep", "/bin/sleep", vec!["30".to_string()], quick());
    assert!(stopped.start_if_available().await);
    stopped.stop().await.unwrap();

    for _ in 0..30 {
        assert_eq!(missing.supervise().await, Supervision::Idle);
        assert_eq!(stopped.supervise().await, Supervision::Idle);
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(!missing.is_running() && !stopped.is_running());
}

#[tokio::test]
async fn stop_after_the_source_exited_succeeds() {
    let mut manager = Manager::with_backoff("true", "/bin/true", vec![], quick());
    assert!(manager.start_if_available().await);
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(manager.stop().await.is_ok());
}

fn proc_state(pid: u32) -> char {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).unwrap();
    stat.rsplit(')')
        .next()
        .unwrap()
        .trim_start()
        .chars()
        .next()
        .unwrap()
}

#[tokio::test]
async fn announcer_pauses_and_resumes_only_running_sources() {
    use audio_player::announcer::Sources;
    use audio_player::manager::ManagedSources;

    let mut running = Manager::new("sleep", "/bin/sleep", vec!["30".to_string()]);
    assert!(running.start_if_available().await);
    let pid = running.pid().unwrap();
    let mut missing = Manager::new("missing", "/nonexistent/source", vec![]);
    assert!(!missing.start_if_available().await);
    let mut sources = ManagedSources {
        managers: vec![running, missing],
        mpd: None,
    };

    let paused = sources.pause_all().await;
    assert_eq!(paused, vec!["sleep".to_string()]);
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(proc_state(pid), 'T', "source should be stopped");
    // A stopped source has not exited, so supervision leaves it alone.
    for (_, event) in sources.supervise_all().await {
        assert_eq!(event, Supervision::Idle);
    }

    sources.resume(&paused).await;
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(proc_state(pid), 'S', "source should run again");
    sources.stop_all().await.unwrap();
}
