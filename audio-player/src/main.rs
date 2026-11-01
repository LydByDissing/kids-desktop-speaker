use audio_player::arbitrator::Arbitrator;
use audio_player::ipc;
use audio_player::manager::Manager;
use std::sync::Arc;
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

    // Wait for Ctrl+C
    tokio::signal::ctrl_c().await?;

    println!("audio-player: shutting down");

    // Stop both subprocesses
    airplay_manager.stop().await?;
    spotify_manager.stop().await?;

    // Cancel the IPC task
    ipc_handle.abort();

    println!("audio-player: stopped");

    Ok(())
}
