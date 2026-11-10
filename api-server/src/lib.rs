pub mod routes;
pub mod state;

use axum::{
    routing::{get, post},
    Router,
};
use state::AppState;

pub fn app(state: AppState) -> Router {
    Router::new()
        .route("/health", get(routes::health::get_health))
        .route("/api/system/time", get(routes::system::get_time))
        .route("/api/audio/source", get(routes::audio::get_source))
        .route("/api/tts", post(routes::tts::post_tts))
        .route("/api/ota/status", get(routes::services::get_ota_status))
        .route("/api/ota/check", post(routes::services::post_ota_check))
        .route("/api/ota/reboot", post(routes::services::post_ota_reboot))
        .route(
            "/api/bluetooth/status",
            get(routes::services::get_bluetooth_status),
        )
        .route(
            "/api/audio/volume-limit",
            get(routes::audio::get_volume_limit).post(routes::audio::set_volume_limit),
        )
        .with_state(state)
}
