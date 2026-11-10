use audio_player::announcer::{espeak_voices, AnnounceQueue, Announcer, Aplay, Chimes};
use audio_player::arbitrator::Arbitrator;
use audio_player::ipc;
use audio_player::manager::{ManagedSources, Manager, Supervision};
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::signal::unix::{signal, SignalKind};
use tokio::sync::Mutex;
use tts_engine::{TtsCache, DEFAULT_TTL};

/// Synthesised speech, kept on the data partition (TTS_REQ_001).
const TTS_CACHE_DIR: &str = "/data/tts-cache";
/// The chimes are regenerated at every start, so they live in /run.
const CHIME_DIR: &str = "/run/kds/chimes";
/// The shared output in /etc/asound.conf (DEC-033).
const OUTPUT_DEVICE: &str = "speaker";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("audio-player: starting");

    let arb = Arc::new(Mutex::new(Arbitrator::new()));

    let mut airplay = Manager::new("shairport-sync", "/usr/bin/shairport-sync", vec![]);
    let mut spotify = Manager::new(
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
    let airplay_up = airplay.start_if_available().await;
    let spotify_up = spotify.start_if_available().await;
    println!("audio-player: started (airplay: {airplay_up}, spotify: {spotify_up})");

    let sources = Arc::new(Mutex::new(ManagedSources {
        managers: vec![airplay, spotify],
    }));

    // Announcements (TTS_REQ_002). If the cache or chimes can't be set up, the
    // speaker still plays music; announce requests are then refused.
    let (queue, queue_rx) = AnnounceQueue::new();
    let announcer = match (
        TtsCache::new(TTS_CACHE_DIR, DEFAULT_TTL),
        Chimes::write(CHIME_DIR.as_ref()),
    ) {
        (Ok(cache), Ok(chimes)) => {
            match cache.evict_expired(SystemTime::now()) {
                Ok(n) if n > 0 => println!("audio-player: removed {n} expired TTS cache entries"),
                Ok(_) => {}
                Err(e) => eprintln!("audio-player: TTS cache eviction failed: {e}"),
            }
            Some(Announcer {
                output: Aplay {
                    device: OUTPUT_DEVICE.into(),
                },
                sources: Arc::clone(&sources),
                cache: Arc::new(cache),
                voices: espeak_voices(),
                chimes,
            })
        }
        (cache, chimes) => {
            if let Err(e) = cache {
                eprintln!("audio-player: TTS cache unavailable: {e}");
            }
            if let Err(e) = chimes {
                eprintln!("audio-player: chimes unavailable: {e}");
            }
            None
        }
    };
    let announcer_handle = announcer.map(|a| tokio::spawn(a.run(queue_rx)));

    let arb_clone = Arc::clone(&arb);
    let ipc_handle = tokio::spawn(async move {
        if let Err(e) = ipc::serve("/run/kds/audio.sock", arb_clone, queue).await {
            eprintln!("IPC server error: {}", e);
        }
    });

    // Restart a source that exits on its own (AUD_REQ_002) until asked to stop.
    let mut sigterm = signal(SignalKind::terminate())?;
    let mut tick = tokio::time::interval(Duration::from_secs(1));
    loop {
        tokio::select! {
            _ = tick.tick() => {
                for (name, event) in sources.lock().await.supervise_all().await {
                    report(name, event);
                }
            }
            _ = tokio::signal::ctrl_c() => break,
            _ = sigterm.recv() => break,
        }
    }

    println!("audio-player: shutting down");
    ipc_handle.abort();
    if let Some(handle) = announcer_handle {
        handle.abort();
    }
    sources.lock().await.stop_all().await?;
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
