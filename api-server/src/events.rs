//! Live updates for the Web UI and other clients (UI_REQ_004, DEC-046).
//!
//! `GET /api/events` is a Server-Sent Events stream. The hub fetches each topic
//! through the same handlers as its REST endpoint, every [`INTERVAL`] while
//! someone subscribes and at once after a change through the API, and sends the
//! topics that changed. A new subscriber first gets every topic.

use axum::{
    extract::{Request, State},
    http::Method,
    middleware::Next,
    response::{
        sse::{Event, KeepAlive, Sse},
        Response,
    },
    Json,
};
use futures_util::stream::{self, Stream, StreamExt};
use serde_json::Value;
use std::collections::{HashMap, VecDeque};
use std::convert::Infallible;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, Notify};

use crate::routes;
use crate::state::AppState;

/// How often the hub looks while someone subscribes.
pub const INTERVAL: Duration = Duration::from_secs(2);
/// A comment this often keeps proxies and phones from closing an idle stream.
pub const KEEP_ALIVE: Duration = Duration::from_secs(15);
/// The topics, each named after what its endpoint returns.
pub const TOPICS: [&str; 5] = ["source", "volume", "timer", "bluetooth", "ota"];
/// A countdown that is this far from where it should be has been restarted.
const DRIFT_S: f64 = 3.0;

#[derive(Debug, Clone, PartialEq)]
pub struct Update {
    pub topic: &'static str,
    pub data: Value,
}

impl Update {
    fn event(&self) -> Event {
        Event::default()
            .event(self.topic)
            .data(self.data.to_string())
    }
}

pub struct EventHub {
    tx: broadcast::Sender<Update>,
    /// The last value sent for each topic, and when.
    last: Mutex<HashMap<&'static str, (Value, Instant)>>,
    nudge: Notify,
    subscribed: Notify,
}

impl Default for EventHub {
    fn default() -> Self {
        let (tx, _) = broadcast::channel(64);
        EventHub {
            tx,
            last: Mutex::default(),
            nudge: Notify::new(),
            subscribed: Notify::new(),
        }
    }
}

/// Whether `new` says the same as `old`, sent `elapsed` ago. Countdowns
/// (`remaining_s`) tick in the client, so they count as the same unless they
/// are more than [`DRIFT_S`] from where `old` would be by now.
pub fn same(old: &Value, new: &Value, elapsed: Duration) -> bool {
    match (old, new) {
        (Value::Object(a), Value::Object(b)) => {
            a.len() == b.len()
                && a.iter().all(|(key, va)| {
                    b.get(key).is_some_and(|vb| {
                        if key == "remaining_s" {
                            match (va.as_f64(), vb.as_f64()) {
                                (Some(x), Some(y)) => {
                                    let expected = (x - elapsed.as_secs_f64()).max(0.0);
                                    (y - expected).abs() <= DRIFT_S
                                }
                                _ => va == vb,
                            }
                        } else {
                            same(va, vb, elapsed)
                        }
                    })
                })
        }
        (Value::Array(a), Value::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(x, y)| same(x, y, elapsed))
        }
        _ => old == new,
    }
}

/// The current value of every topic. A topic whose service doesn't answer is
/// left out.
async fn fetch(state: &AppState) -> Vec<(&'static str, Value)> {
    let ok = |r: Result<Json<Value>, _>| r.ok().map(|Json(v)| v);
    let (source, volume, timer, bluetooth, ota) = tokio::join!(
        routes::audio::get_source(State(state.clone())),
        routes::audio::get_volume(State(state.clone())),
        routes::schedule::get_timer(State(state.clone())),
        routes::services::get_bluetooth_status(State(state.clone())),
        routes::services::get_ota_status(State(state.clone())),
    );
    [
        ("source", ok(source)),
        ("volume", ok(volume)),
        ("timer", Some(timer.0)),
        ("bluetooth", ok(bluetooth)),
        ("ota", ok(ota)),
    ]
    .into_iter()
    .filter_map(|(topic, value)| value.map(|v| (topic, v)))
    .collect()
}

impl EventHub {
    pub fn new() -> Self {
        Self::default()
    }

    /// Looks now instead of at the next interval.
    pub fn nudge(&self) {
        self.nudge.notify_one();
    }

    /// The last value of every topic, as sent.
    pub fn snapshot(&self) -> Vec<Update> {
        let last = self.last.lock().expect("event hub");
        TOPICS
            .iter()
            .filter_map(|t| {
                last.get(t).map(|(data, _)| Update {
                    topic: t,
                    data: data.clone(),
                })
            })
            .collect()
    }

    /// Fetches every topic, sends those that changed and returns them.
    pub async fn refresh(&self, state: &AppState) -> Vec<Update> {
        let current = fetch(state).await;
        let now = Instant::now();
        let mut changed = Vec::new();
        {
            let mut last = self.last.lock().expect("event hub");
            let answered: Vec<&str> = current.iter().map(|(t, _)| *t).collect();
            // A service that stopped answering is sent again when it's back.
            last.retain(|t, _| answered.contains(t));
            for (topic, data) in current {
                let unchanged = last
                    .get(topic)
                    .is_some_and(|(old, at)| same(old, &data, now - *at));
                if !unchanged {
                    last.insert(topic, (data.clone(), now));
                    changed.push(Update { topic, data });
                }
            }
        }
        for update in &changed {
            let _ = self.tx.send(update.clone());
        }
        changed
    }

    /// A new subscriber: its receiver and the current value of every topic.
    pub async fn subscribe(&self, state: &AppState) -> (broadcast::Receiver<Update>, Vec<Update>) {
        // Refresh before subscribing, so the subscriber doesn't also get this
        // refresh's changes; a change after it is in the snapshot or on `rx`.
        self.refresh(state).await;
        let rx = self.tx.subscribe();
        self.subscribed.notify_one();
        (rx, self.snapshot())
    }

    pub fn subscribers(&self) -> usize {
        self.tx.receiver_count()
    }
}

/// Watches the topics while anyone subscribes. Runs for the life of the server.
pub async fn watch(state: AppState, every: Duration) {
    let hub = Arc::clone(&state.events);
    loop {
        if hub.subscribers() == 0 {
            hub.subscribed.notified().await;
            continue;
        }
        hub.refresh(&state).await;
        tokio::select! {
            _ = tokio::time::sleep(every) => {}
            _ = hub.nudge.notified() => {
                // Let the backend act on the change before looking.
                tokio::time::sleep(Duration::from_millis(300)).await;
            }
        }
    }
}

/// `GET /api/events`: every topic, then each change (DEC-046).
pub async fn stream(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let hub = Arc::clone(&state.events);
    let (rx, snapshot) = hub.subscribe(&state).await;
    let pending: VecDeque<Update> = snapshot.into();
    let events = stream::unfold(
        (rx, hub, pending),
        |(mut rx, hub, mut pending)| async move {
            loop {
                if let Some(update) = pending.pop_front() {
                    return Some((update, (rx, hub, pending)));
                }
                match rx.recv().await {
                    Ok(update) => return Some((update, (rx, hub, pending))),
                    // Fell behind: skip to the latest values.
                    Err(broadcast::error::RecvError::Lagged(_)) => pending = hub.snapshot().into(),
                    Err(broadcast::error::RecvError::Closed) => return None,
                }
            }
        },
    )
    .map(|update| Ok(update.event()));
    Sse::new(events).keep_alive(KeepAlive::new().interval(KEEP_ALIVE))
}

/// After a successful change through the API, look at once, so the sender's
/// own change reaches every subscriber without waiting for the interval.
pub async fn nudge_after_changes(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    let change = request.method() != Method::GET && request.uri().path().starts_with("/api/");
    let response = next.run(request).await;
    if change && response.status().is_success() {
        state.events.nudge();
    }
    response
}
