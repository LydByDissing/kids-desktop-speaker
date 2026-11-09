use audio_player::arbitrator::Arbitrator;
use audio_player::ipc;
use audio_player::manager::{Manager, Supervision};
use std::sync::Arc;
use std::time::Duration;
use tokio::signal::unix::{signal, SignalKind};
use tokio::sync::Mutex;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("audio-player: starting");

    // Create the arbitrator
    let arb = Arc::new(Mutex::new(Arbitrator::new()));

    // Create managers for both services
    let mut airplay_manager = Manager::new("shairport-sync", "/usr/bin/shairport-sync", vec![]);
    let mut spotify_manager = Manager::new(
        "librespot",
        "/usr/bin/librespot",
        vec![
            "--name".to_string(),
            "KidsSpeaker".to_string(),
            "--backend".to_string(),
            "alsa".to_string(),
        ],
    );

    // Start whichever sources are installed
    let airplay = airplay_manager.start_if_available().await;
    let spotify = spotify_manager.start_if_available().await;

    println!("audio-player: started (airplay: {airplay}, spotify: {spotify})");

    // Spawn IPC server task
    let arb_clone = Arc::clone(&arb);
    let ipc_handle = tokio::spawn(async move {
        if let Err(e) = ipc::serve("/run/kds/audio.sock", arb_clone).await {
            eprintln!("IPC server error: {}", e);
        }
    });

    // Restart a source that exits on its own (AUD_REQ_002) until asked to stop.
    let mut sigterm = signal(SignalKind::terminate())?;
    let mut tick = tokio::time::interval(Duration::from_secs(1));
    loop {
        tokio::select! {
            _ = tick.tick() => {
                for manager in [&mut airplay_manager, &mut spotify_manager] {
                    report(manager.name(), manager.supervise().await);
                }
            }
            _ = tokio::signal::ctrl_c() => break,
            _ = sigterm.recv() => break,
        }
    }

    println!("audio-player: shutting down");

    // Stop both subprocesses
    airplay_manager.stop().await?;
    spotify_manager.stop().await?;

    // Cancel the IPC task
    ipc_handle.abort();

    println!("audio-player: stopped");

    Ok(())
}

fn report(name: &str, event: Supervision) {
    match event {
        Supervision::Idle => {}
        Supervision::Exited { status, restart_in } => {
            eprintln!("audio-player: {name} exited ({status}); restarting in {restart_in:?}")
        }
        Supervision::Restarted => println!("audio-player: {name} restarted"),
        Supervision::RestartFailed { error, retry_in } => {
            eprintln!("audio-player: {error}; retrying in {retry_in:?}")
        }
    }
}
