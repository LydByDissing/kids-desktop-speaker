use axum::response::Json;
use serde_json::{json, Value};

pub async fn get_health() -> Json<Value> {
    Json(json!({
        "status": "ok",
        "version": "0.1.0"
    }))
}
