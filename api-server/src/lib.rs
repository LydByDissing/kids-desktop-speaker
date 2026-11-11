pub mod auth;
pub mod messages;
pub mod routes;
pub mod setup;
pub mod state;
pub mod tls;
pub mod transport;

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
        .route(
            "/api/radio/stations",
            get(routes::radio::get_stations).post(routes::radio::add_station),
        )
        .route(
            "/api/radio/stations/:id",
            axum::routing::delete(routes::radio::remove_station),
        )
        .route(
            "/api/messages",
            get(routes::messages::list)
                .post(routes::messages::upload)
                .layer(axum::extract::DefaultBodyLimit::max(messages::MAX_BYTES)),
        )
        .route(
            "/api/messages/:id",
            axum::routing::patch(routes::messages::rename).delete(routes::messages::delete),
        )
        .route("/api/messages/:id/audio", get(routes::messages::audio))
        .route("/api/messages/:id/play", post(routes::messages::replay))
        .route("/api/radio/play", post(routes::radio::play))
        .route("/api/radio/stop", post(routes::radio::stop))
        .route("/api/ota/status", get(routes::services::get_ota_status))
        .route("/api/ota/check", post(routes::services::post_ota_check))
        .route("/api/ota/reboot", post(routes::services::post_ota_reboot))
        .route(
            "/api/bluetooth/status",
            get(routes::services::get_bluetooth_status),
        )
        .route(
            "/api/bluetooth/pair",
            post(routes::services::post_bluetooth_pair),
        )
        .route(
            "/api/audio/volume-limit",
            get(routes::audio::get_volume_limit).post(routes::audio::set_volume_limit),
        )
        .route("/api/setup/start", post(routes::setup::start))
        .route("/api/setup", post(routes::setup::complete))
        .merge(
            Router::new()
                .route(
                    "/api/tokens",
                    get(routes::tokens::list).post(routes::tokens::create),
                )
                .route(
                    "/api/tokens/:id",
                    axum::routing::delete(routes::tokens::revoke),
                )
                .route_layer(axum::middleware::from_fn(auth::require_admin)),
        )
        // Every route needs a token except /health, setup and the device itself
        // (AUTH_REQ_001). The listener's Transport and the peer address are
        // added outside this, in main.
        .route_layer(axum::middleware::from_fn_with_state(
            state.clone(),
            auth::authenticate,
        ))
        .with_state(state)
}

/// The API as a process on the device sees it: over loopback HTTP, with full
/// access. For tests of the routes themselves.
pub fn local_app(state: AppState) -> Router {
    app(state)
        .layer(axum::Extension(transport::Transport::Http))
        .layer(axum::extract::connect_info::MockConnectInfo(
            std::net::SocketAddr::from(([127, 0, 0, 1], 9)),
        ))
}
