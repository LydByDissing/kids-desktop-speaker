pub mod routes;
pub mod state;

use axum::{routing::get, Router};
use state::AppState;

pub fn app(state: AppState) -> Router {
    Router::new()
        .route("/health", get(routes::health::get_health))
        .route("/api/system/time", get(routes::system::get_time))
        .route("/api/audio/source", get(routes::audio::get_source))
        .route(
            "/api/audio/volume-limit",
            get(routes::audio::get_volume_limit).post(routes::audio::set_volume_limit),
        )
        .with_state(state)
}
