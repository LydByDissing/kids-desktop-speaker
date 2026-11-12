//! Plays a URL through the stream player for a while and prints its status:
//! `play_url <url> [seconds] [device] [seek]`. For measuring on the Pi.

use audio_player::stream::Player;
use std::time::Duration;

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let url = args
        .get(1)
        .expect("usage: play_url <url> [seconds] [device] [seek]");
    let seconds: u64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(20);
    let device = args.get(3).map(String::as_str).unwrap_or("speaker");
    let player = Player::aplay(device);
    let started = std::time::Instant::now();
    if let Err(e) = player.play_url(url).await {
        eprintln!("{e}");
        std::process::exit(1);
    }
    println!("opened in {:.1} s", started.elapsed().as_secs_f64());
    if let Some(seek) = args.get(4).and_then(|s| s.parse().ok()) {
        player.seek(seek).await.unwrap();
    }
    for _ in 0..seconds / 2 {
        tokio::time::sleep(Duration::from_secs(2)).await;
        println!("{:?}", player.status().await.unwrap());
    }
    player.stop().await.unwrap();
}
