//! Token management (AUTH_REQ_002): admin only, over HTTPS.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::state::AppState;

type Reply = Result<(StatusCode, Json<Value>), (StatusCode, Json<Value>)>;

pub async fn list(State(state): State<AppState>) -> Json<Value> {
    Json(json!({ "tokens": state.tokens.list() }))
}

#[derive(Debug, Deserialize)]
pub struct NewToken {
    pub name: String,
    #[serde(default)]
    pub admin: bool,
}

/// Creates a token; the token itself is shown only in this reply.
pub async fn create(State(state): State<AppState>, Json(body): Json<NewToken>) -> Reply {
    let (info, token) = state
        .tokens
        .create(&body.name, body.admin)
        .map_err(|e| (StatusCode::BAD_REQUEST, Json(json!({"error": e}))))?;
    Ok((
        StatusCode::CREATED,
        Json(json!({"token": token, "info": info})),
    ))
}

pub async fn revoke(State(state): State<AppState>, Path(id): Path<String>) -> Reply {
    state.tokens.revoke(&id).map_err(|e| {
        let status = if e.starts_with("no token") {
            StatusCode::NOT_FOUND
        } else {
            StatusCode::CONFLICT
        };
        (status, Json(json!({"error": e})))
    })?;
    Ok((StatusCode::OK, Json(json!({}))))
}
