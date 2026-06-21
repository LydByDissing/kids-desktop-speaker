use axum::response::Json;
use chrono::Utc;
use serde_json::{json, Value};
use std::path::Path;

pub async fn get_time() -> Json<Value> {
    let now = Utc::now();
    let synced = Path::new("/run/systemd/timesync/synchronized").exists();
    Json(json!({
        "synced": synced,
        "time": now.to_rfc3339()
    }))
}
