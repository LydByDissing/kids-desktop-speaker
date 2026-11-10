//! Times a cache miss and a hit with a real engine.
//! Usage: cache_demo <cache-dir> <flite|espeak-ng> <voice> <text>

use std::time::{Instant, SystemTime};
use tts_engine::{Engine, TtsCache, DEFAULT_TTL};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let [_, dir, engine, voice, text] = args.as_slice() else {
        eprintln!("usage: cache_demo <cache-dir> <flite|espeak-ng> <voice> <text>");
        std::process::exit(2);
    };
    let engine = match engine.as_str() {
        "flite" => Engine::Flite {
            voice: voice.clone(),
        },
        _ => Engine::EspeakNg {
            voice: voice.clone(),
        },
    };
    let cache = TtsCache::new(dir, DEFAULT_TTL).expect("cache dir");
    println!(
        "expired entries removed: {}",
        cache.evict_expired(SystemTime::now()).unwrap()
    );
    for attempt in 1..=2 {
        let start = Instant::now();
        let (path, lookup) = cache.get_or_synthesize(&engine, text).expect("synthesis");
        println!(
            "attempt {attempt}: {lookup:?} in {} ms -> {} ({} bytes)",
            start.elapsed().as_millis(),
            path.display(),
            std::fs::metadata(&path).unwrap().len()
        );
    }
}
