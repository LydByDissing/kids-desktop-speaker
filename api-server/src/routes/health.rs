use axum::{extract::State, response::Json};
use serde_json::{json, Value};

use crate::state::AppState;

/// No token needed. `set_up` tells the Web UI whether to show first-time setup
/// (AUTH_REQ_003); the speaker reading its code aloud says as much.
pub async fn get_health(State(state): State<AppState>) -> Json<Value> {
    Json(json!({
        "status": "ok",
        "version": "0.1.0",
        "set_up": state.tokens.has_admin(),
    }))
}
