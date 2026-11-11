//! /api/timer and /api/alarms (SLP_REQ_001, ALM_REQ_001, DEC-021, DEC-041).

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::time::Duration;

use crate::schedule::{run_timer, AlarmInput, MAX_TIMER_MINUTES};
use crate::state::AppState;

type Reply = Result<(StatusCode, Json<Value>), (StatusCode, Json<Value>)>;

fn store_error(e: String) -> (StatusCode, Json<Value>) {
    let status = if e.starts_with("no alarm") {
        StatusCode::NOT_FOUND
    } else {
        StatusCode::BAD_REQUEST
    };
    (status, Json(json!({ "error": e })))
}

fn timer_json(state: &AppState) -> Value {
    match state.sleep.remaining() {
        Some((left, minutes)) => {
            json!({ "running": true, "minutes": minutes, "remaining_s": left })
        }
        None => json!({ "running": false }),
    }
}

pub async fn get_timer(State(state): State<AppState>) -> Json<Value> {
    Json(timer_json(&state))
}

#[derive(Debug, Deserialize)]
pub struct TimerBody {
    pub minutes: u32,
}

/// Starts the sleep timer, replacing a running one.
pub async fn start_timer(State(state): State<AppState>, Json(body): Json<TimerBody>) -> Reply {
    if body.minutes == 0 || body.minutes > MAX_TIMER_MINUTES {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": format!("minutes must be 1 to {MAX_TIMER_MINUTES}") })),
        ));
    }
    let generation = state.sleep.start(body.minutes);
    let length = Duration::from_secs(body.minutes as u64 * 60);
    tokio::spawn(run_timer(
        state.clone(),
        generation,
        length,
        Duration::from_secs(30),
    ));
    Ok((StatusCode::OK, Json(timer_json(&state))))
}

pub async fn cancel_timer(State(state): State<AppState>) -> Json<Value> {
    state.sleep.cancel();
    Json(timer_json(&state))
}

pub async fn list_alarms(State(state): State<AppState>) -> Reply {
    let alarms = state.alarms.list().map_err(store_error)?;
    Ok((StatusCode::OK, Json(json!({ "alarms": alarms }))))
}

pub async fn add_alarm(State(state): State<AppState>, Json(body): Json<AlarmInput>) -> Reply {
    let alarm = state.alarms.add(body).map_err(store_error)?;
    Ok((StatusCode::CREATED, Json(json!({ "alarm": alarm }))))
}

pub async fn update_alarm(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<AlarmInput>,
) -> Reply {
    let alarm = state.alarms.update(&id, body).map_err(store_error)?;
    Ok((StatusCode::OK, Json(json!({ "alarm": alarm }))))
}

pub async fn remove_alarm(State(state): State<AppState>, Path(id): Path<String>) -> Reply {
    state.alarms.remove(&id).map_err(store_error)?;
    Ok((StatusCode::OK, Json(json!({}))))
}
