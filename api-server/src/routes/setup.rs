//! Setup with a spoken code (AUTH_REQ_003, DEC-038).

use axum::{extract::State, http::StatusCode, response::Json};
use serde::Deserialize;
use serde_json::{json, Value};
use std::time::{Duration, Instant};

use crate::routes::services::send;
use crate::setup::{spoken, CheckError, StartError, CODE_LIFETIME};
use crate::state::{AppState, Service};

type Reply = Result<(StatusCode, Json<Value>), (StatusCode, Json<Value>)>;

fn fail(status: StatusCode, body: Value) -> (StatusCode, Json<Value>) {
    (status, Json(body))
}

/// Has the speaker read a code aloud (queued in the announcement queue).
async fn read_aloud(state: &AppState, code: &str) -> Result<(), (StatusCode, Json<Value>)> {
    send(
        state,
        Service::AudioPlayer,
        &json!({"cmd": "announce", "text": spoken(code), "lang": "da", "slow": true}),
    )
    .await
    .map(|_| ())
    .map_err(|_| {
        fail(
            StatusCode::SERVICE_UNAVAILABLE,
            json!({"error": "the speaker could not read the code"}),
        )
    })
}

/// Asks the speaker to read a new code (to set it up, or to recover a lost
/// admin token). At most once a minute.
pub async fn start(State(state): State<AppState>) -> Reply {
    let code = state.setup.request_new(Instant::now()).map_err(|StartError::TooSoon(wait)| {
        fail(
            StatusCode::TOO_MANY_REQUESTS,
            json!({"error": "a code was requested recently", "retry_after_s": wait.as_secs() + 1}),
        )
    })?;
    read_aloud(&state, &code).await?;
    Ok((
        StatusCode::ACCEPTED,
        Json(json!({"expires_in_s": CODE_LIFETIME.as_secs()})),
    ))
}

#[derive(Debug, Deserialize)]
pub struct Complete {
    pub code: String,
    /// What to call the admin token, such as the parent's phone.
    pub name: String,
}

/// Exchanges the spoken code for an admin token, shown only in this reply.
pub async fn complete(State(state): State<AppState>, Json(body): Json<Complete>) -> Reply {
    match state.setup.check(&body.code, Instant::now()) {
        Ok(()) => {
            let (info, token) = state
                .tokens
                .create(&body.name, true)
                .map_err(|e| fail(StatusCode::BAD_REQUEST, json!({"error": e})))?;
            println!(
                "api-server: setup completed; admin token {:?} created",
                info.name
            );
            Ok((
                StatusCode::CREATED,
                Json(json!({"token": token, "info": info})),
            ))
        }
        Err(CheckError::NoCode) => Err(fail(
            StatusCode::CONFLICT,
            json!({"error": "no setup code is active; ask the speaker for one"}),
        )),
        Err(CheckError::Wrong(left)) => Err(fail(
            StatusCode::FORBIDDEN,
            json!({"error": "wrong code", "attempts_left": left}),
        )),
    }
}

/// Until the speaker is set up, it reads the setup code after start-up and
/// again 8 s after each reading ends (AUTH_REQ_003).
pub async fn read_until_set_up(state: AppState) {
    read_until_set_up_every(state, Duration::from_secs(8)).await
}

/// As [`read_until_set_up`], with the pause after each reading as a parameter.
pub async fn read_until_set_up_every(state: AppState, pause: Duration) {
    let mut warned = false;
    while !state.tokens.has_admin() {
        let code = state.setup.current_or_new(Instant::now(), true);
        match read_aloud(&state, &code).await {
            Ok(()) => {
                warned = false;
                // Wait for this reading to finish before the pause.
                for _ in 0..120 {
                    tokio::time::sleep(Duration::from_millis(500)).await;
                    let status =
                        send(&state, Service::AudioPlayer, &json!({"cmd": "status"})).await;
                    if !status.is_ok_and(|s| s["announcing"] == true) {
                        break;
                    }
                }
            }
            Err(_) if !warned => {
                eprintln!("api-server: not set up, and the speaker cannot read the setup code yet");
                warned = true;
            }
            Err(_) => {}
        }
        tokio::time::sleep(pause).await;
    }
}
