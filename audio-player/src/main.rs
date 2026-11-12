use audio_player::announcer::{
    espeak_voices, slow_espeak_voices, AnnounceQueue, Announcer, Aplay, Chimes,
};
use audio_player::arbitrator::Arbitrator;
use audio_player::ipc::{self, Radio, Services};
use audio_player::manager::{ManagedSources, Supervision};
use audio_player::podcasts::{PodcastStore, Podcasts, PODCASTS_DIR};
use audio_player::radio::{StationStore, STATIONS_FILE};
use audio_player::sources;
use audio_player::stream::{PlayState, Player};
use audio_player::switcher::{BtClient, DeviceStopper};
use audio_player::volume::{Camilla, Volume, VolumeStore, CAMILLA_ADDR, VOLUME_FILE};
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::signal::unix::{signal, SignalKind};
use tokio::sync::Mutex;
use tts_engine::{TtsCache, DEFAULT_TTL};

/// Synthesised speech, kept on the data partition (TTS_REQ_001).
const TTS_CACHE_DIR: &str = "/data/tts-cache";
/// The chimes are regenerated at every start, so they live in /run.
const CHIME_DIR: &str = "/run/kds/chimes";
/// Recorded voice messages, written by the API Server (DEC-037).
const MESSAGES_DIR: &str = "/data/messages";
/// The shared output in /etc/asound.conf (DEC-033).
const OUTPUT_DEVICE: &str = "speaker";
const AUDIO_SOCKET: &str = "/run/kds/audio.sock";
const BT_SOCKET: &str = "/run/kds/bt.sock";

/// `audio-player notify <source> <event>`: what the receivers' hooks run to
/// report that they started or stopped playing (DEC-039).
async fn notify(source: &str, event: &str) -> anyhow::Result<()> {
    let request = serde_json::json!({"cmd": "source_event", "source": source, "state": event});
    kds_ipc::request(AUDIO_SOCKET, &request).await?;
    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("notify") {
        let (Some(source), Some(event)) = (args.get(2), args.get(3)) else {
            anyhow::bail!("usage: audio-player notify <source> <event>");
        };
        return notify(source, event).await;
    }
    println!("audio-player: starting");

    let arb = Arc::new(Mutex::new(Arbitrator::new()));

    let name = sources::device_name(&sources::hostname());
    let mut airplay = sources::airplay();
    let mut spotify = sources::spotify(&name);
    let mut bluetooth = sources::bluetooth();

    // Start whichever sources are installed
    let airplay_up = airplay.start_if_available().await;
    let spotify_up = spotify.start_if_available().await;
    let bluetooth_up = bluetooth.start_if_available().await;
    println!("audio-player: started as {name:?} (airplay: {airplay_up}, spotify: {spotify_up}, bluetooth: {bluetooth_up})");

    // Internet radio and podcasts through the stream player (RAD_REQ_001,
    // DEC-048), on the shared output like announcements.
    let player = Player::aplay(OUTPUT_DEVICE);
    let radio = Arc::new(Radio {
        stations: StationStore::new(STATIONS_FILE),
        player: player.clone(),
        current: std::sync::Mutex::new(None),
    });

    let podcasts = Arc::new(Podcasts {
        store: PodcastStore::new(PODCASTS_DIR),
        player: player.clone(),
        current: std::sync::Mutex::new(None),
    });
    // Keep the listening position of a playing podcast episode (POD_REQ_002).
    let saver = Arc::clone(&podcasts);
    tokio::spawn(async move {
        let mut every = tokio::time::interval(Duration::from_secs(15));
        loop {
            every.tick().await;
            if saver.current().is_some()
                && saver
                    .player
                    .status()
                    .await
                    .is_ok_and(|s| s.state == PlayState::Play)
            {
                saver.save_current().await;
            }
        }
    });

    // The device volume and limit (VOL_REQ_001, DEC-040), applied once
    // CamillaDSP answers.
    let store = VolumeStore::new(VOLUME_FILE);
    let volume = Arc::new(Volume {
        settings: std::sync::Mutex::new(store.load()),
        store,
        camilla: Camilla {
            addr: CAMILLA_ADDR.into(),
        },
    });
    let applier = Arc::clone(&volume);
    tokio::spawn(async move { applier.apply_when_ready().await });

    let bt = BtClient {
        socket: BT_SOCKET.into(),
    };
    let sources = Arc::new(Mutex::new(ManagedSources {
        managers: vec![airplay, spotify, bluetooth],
        player: Some(player),
        bt: Some(bt.clone()),
    }));
    // When a source starts, the others stop (AUD_REQ_003, DEC-039).
    let stopper = Arc::new(DeviceStopper {
        sources: Arc::clone(&sources),
        radio: Some(Arc::clone(&radio)),
        podcasts: Some(Arc::clone(&podcasts)),
        bt,
    });

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
                slow_voices: slow_espeak_voices(),
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
    let announcing = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let busy = Arc::clone(&announcing);
    let announcer_handle = announcer.map(|a| tokio::spawn(a.run_tracked(queue_rx, busy)));

    let arb_clone = Arc::clone(&arb);
    let ipc_handle = tokio::spawn(async move {
        let services = Services {
            announcements: Some(queue),
            radio: Some(radio),
            messages_dir: Some(MESSAGES_DIR.into()),
            announcing: Some(announcing),
            stopper: Some(stopper),
            podcasts: Some(podcasts),
            volume: Some(volume),
        };
        if let Err(e) = ipc::serve(AUDIO_SOCKET, arb_clone, services).await {
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
