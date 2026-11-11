//! The sleep timer and alarms (SLP_REQ_001, ALM_REQ_001, DEC-021, DEC-041).
//! The API Server schedules them and drives the Audio Player over IPC.

use chrono::{Datelike, Duration as ChronoDuration, NaiveDateTime, NaiveTime};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::routes::services::send;
use crate::state::{AppState, Service};

pub const ALARMS_FILE: &str = "/data/api/alarms.json";
/// The scheduler never catches up more than this, so setting the clock after
/// boot doesn't fire a morning's alarms at once.
pub const MAX_CATCH_UP: ChronoDuration = ChronoDuration::minutes(2);
pub const MAX_TIMER_MINUTES: u32 = 240;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Alarm {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    /// Local time of day, "HH:MM".
    pub time: String,
    /// ISO weekdays it repeats on (1 = Monday ... 7 = Sunday); empty is every day.
    #[serde(default)]
    pub days: Vec<u8>,
    /// A radio station id (RAD_REQ_001).
    pub station: String,
    /// The volume it rises to, capped by the limit (VOL_REQ_001).
    pub volume: u8,
    #[serde(default = "enabled_default")]
    pub enabled: bool,
}

fn enabled_default() -> bool {
    true
}

/// The fields a client sends to create or change an alarm.
#[derive(Debug, Clone, Deserialize)]
pub struct AlarmInput {
    pub name: Option<String>,
    pub time: String,
    #[serde(default)]
    pub days: Vec<u8>,
    pub station: String,
    pub volume: u8,
    #[serde(default = "enabled_default")]
    pub enabled: bool,
}

pub fn parse_time(text: &str) -> Option<NaiveTime> {
    NaiveTime::parse_from_str(text.trim(), "%H:%M").ok()
}

impl AlarmInput {
    fn validate(&self) -> Result<(), String> {
        parse_time(&self.time).ok_or("time must be HH:MM")?;
        if self.days.iter().any(|d| !(1..=7).contains(d)) {
            return Err("days must be 1 (Monday) to 7 (Sunday)".into());
        }
        if self.volume == 0 || self.volume > 100 {
            return Err("volume must be 1 to 100".into());
        }
        if self.station.trim().is_empty() {
            return Err("station is required".into());
        }
        if self
            .name
            .as_deref()
            .is_some_and(|n| n.chars().count() > 100)
        {
            return Err("name must be at most 100 characters".into());
        }
        Ok(())
    }

    fn into_alarm(self, id: String) -> Alarm {
        let mut days = self.days;
        days.sort_unstable();
        days.dedup();
        Alarm {
            id,
            name: self
                .name
                .map(|n| n.trim().to_string())
                .filter(|n| !n.is_empty()),
            time: self.time.trim().to_string(),
            days,
            station: self.station.trim().to_string(),
            volume: self.volume,
            enabled: self.enabled,
        }
    }
}

/// Whether `alarm` should go off for the moment the clock moved from `from` to
/// `to` (local time). At most [`MAX_CATCH_UP`] back from `to` is considered.
pub fn fires_between(alarm: &Alarm, from: NaiveDateTime, to: NaiveDateTime) -> bool {
    if !alarm.enabled || to <= from {
        return false;
    }
    let Some(time) = parse_time(&alarm.time) else {
        return false;
    };
    let from = from.max(to - MAX_CATCH_UP);
    [from.date(), to.date()].iter().any(|date| {
        let at = date.and_time(time);
        let weekday = date.weekday().number_from_monday() as u8;
        from < at && at <= to && (alarm.days.is_empty() || alarm.days.contains(&weekday))
    })
}

pub struct AlarmStore {
    path: PathBuf,
}

impl AlarmStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        AlarmStore { path: path.into() }
    }

    pub fn list(&self) -> Result<Vec<Alarm>, String> {
        match std::fs::read(&self.path) {
            Ok(bytes) => {
                serde_json::from_slice(&bytes).map_err(|e| format!("alarms are damaged: {e}"))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(e) => Err(format!("cannot read alarms: {e}")),
        }
    }

    fn save(&self, alarms: &[Alarm]) -> Result<(), String> {
        let dir = self.path.parent().map(PathBuf::from).unwrap_or_default();
        std::fs::create_dir_all(&dir).map_err(|e| format!("cannot save alarms: {e}"))?;
        let temp = self.path.with_extension("json.tmp");
        let json = serde_json::to_vec_pretty(alarms).map_err(|e| e.to_string())?;
        std::fs::write(&temp, json)
            .and_then(|()| std::fs::rename(&temp, &self.path))
            .map_err(|e| format!("cannot save alarms: {e}"))
    }

    pub fn add(&self, input: AlarmInput) -> Result<Alarm, String> {
        input.validate()?;
        let mut alarms = self.list()?;
        if alarms.len() >= 20 {
            return Err("too many alarms (20)".into());
        }
        let id = (1..)
            .map(|n| format!("alarm-{n}"))
            .find(|id| !alarms.iter().any(|a| &a.id == id))
            .unwrap();
        let alarm = input.into_alarm(id);
        alarms.push(alarm.clone());
        self.save(&alarms)?;
        Ok(alarm)
    }

    pub fn update(&self, id: &str, input: AlarmInput) -> Result<Alarm, String> {
        input.validate()?;
        let mut alarms = self.list()?;
        let slot = alarms
            .iter_mut()
            .find(|a| a.id == id)
            .ok_or_else(|| format!("no alarm {id}"))?;
        *slot = input.into_alarm(id.to_string());
        let alarm = slot.clone();
        self.save(&alarms)?;
        Ok(alarm)
    }

    pub fn remove(&self, id: &str) -> Result<(), String> {
        let mut alarms = self.list()?;
        let before = alarms.len();
        alarms.retain(|a| a.id != id);
        if alarms.len() == before {
            return Err(format!("no alarm {id}"));
        }
        self.save(&alarms)
    }
}

async fn audio(state: &AppState, request: Value) -> Result<Value, String> {
    send(state, Service::AudioPlayer, &request)
        .await
        .map_err(|(_, body)| {
            body.0["error"]
                .as_str()
                .unwrap_or("audio player unavailable")
                .to_string()
        })
}

async fn set_volume(state: &AppState, volume: u8) -> Result<(), String> {
    audio(state, json!({"cmd": "set_volume", "volume": volume}))
        .await
        .map(|_| ())
}

/// Sets off an alarm: the station starts at half the alarm's volume and rises
/// to it over `ramp`; without the station, the speaker says "Godmorgen".
pub async fn fire(state: &AppState, alarm: &Alarm, ramp: Duration) {
    println!("api-server: alarm {} ({}) goes off", alarm.id, alarm.time);
    let start = (alarm.volume / 2).max(1);
    let _ = set_volume(state, start).await;
    if let Err(e) = audio(state, json!({"cmd": "radio_play", "id": alarm.station})).await {
        eprintln!(
            "api-server: alarm {} could not play {}: {e}",
            alarm.id, alarm.station
        );
        let _ = set_volume(state, alarm.volume).await;
        let _ = audio(
            state,
            json!({"cmd": "announce", "text": "Godmorgen", "lang": "da"}),
        )
        .await;
        return;
    }
    const STEPS: u32 = 12;
    for step in 1..=STEPS {
        tokio::time::sleep(ramp / STEPS).await;
        let v = start as u32 + (alarm.volume as u32 - start as u32) * step / STEPS;
        let _ = set_volume(state, v as u8).await;
    }
}

/// Checks the alarms every `every` and sets off those that are due.
pub async fn run_alarms(state: AppState, every: Duration) {
    let mut last = chrono::Local::now().naive_local();
    loop {
        tokio::time::sleep(every).await;
        let now = chrono::Local::now().naive_local();
        if let Ok(alarms) = state.alarms.list() {
            for alarm in alarms.into_iter().filter(|a| fires_between(a, last, now)) {
                let state = state.clone();
                tokio::spawn(async move { fire(&state, &alarm, Duration::from_secs(60)).await });
            }
        }
        last = now;
    }
}

/// The running sleep timer, if any.
#[derive(Default)]
pub struct SleepTimer {
    inner: Mutex<(u64, Option<(Instant, u32)>)>,
}

impl SleepTimer {
    /// Starts a timer, replacing a running one. Returns its generation.
    pub fn start(&self, minutes: u32) -> u64 {
        let mut inner = self.inner.lock().expect("timer state");
        inner.0 += 1;
        inner.1 = Some((
            Instant::now() + Duration::from_secs(minutes as u64 * 60),
            minutes,
        ));
        inner.0
    }

    pub fn cancel(&self) -> bool {
        let mut inner = self.inner.lock().expect("timer state");
        inner.0 += 1;
        inner.1.take().is_some()
    }

    /// Seconds left and the minutes it was set to, while one runs.
    pub fn remaining(&self) -> Option<(u64, u32)> {
        let inner = self.inner.lock().expect("timer state");
        inner.1.map(|(end, minutes)| {
            (
                end.saturating_duration_since(Instant::now()).as_secs(),
                minutes,
            )
        })
    }

    /// True if `generation` is still the running timer (not replaced or cancelled).
    pub fn is_current(&self, generation: u64) -> bool {
        let inner = self.inner.lock().expect("timer state");
        inner.0 == generation && inner.1.is_some()
    }

    fn finish(&self, generation: u64) {
        let mut inner = self.inner.lock().expect("timer state");
        if inner.0 == generation {
            inner.1 = None;
        }
    }
}

/// Waits out a timer; then fades the volume to 0 over `fade`, stops everything
/// and restores the volume (SLP_REQ_001). Does nothing if it was replaced or
/// cancelled meanwhile.
pub async fn run_timer(state: AppState, generation: u64, length: Duration, fade: Duration) {
    tokio::time::sleep(length).await;
    if !state.sleep.is_current(generation) {
        return;
    }
    println!("api-server: sleep timer ended; fading out");
    let before = audio(&state, json!({"cmd": "volume"}))
        .await
        .ok()
        .and_then(|v| v["volume"].as_u64())
        .unwrap_or(0) as u8;
    const STEPS: u32 = 10;
    for step in 1..=STEPS {
        if !state.sleep.is_current(generation) {
            let _ = set_volume(&state, before).await;
            return;
        }
        let _ = set_volume(&state, (before as u32 * (STEPS - step) / STEPS) as u8).await;
        tokio::time::sleep(fade / STEPS).await;
    }
    let _ = audio(&state, json!({"cmd": "stop_all"})).await;
    let _ = set_volume(&state, before).await;
    state.sleep.finish(generation);
}
