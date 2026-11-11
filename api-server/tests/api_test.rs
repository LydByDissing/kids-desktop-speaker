use api_server::local_app as app;
use api_server::state::AppState;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

fn make_app() -> axum::Router {
    app(AppState::new())
}

#[tokio::test]
async fn test_health() {
    let app = make_app();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "ok");
    assert_eq!(json["version"], "0.1.0");
}

#[tokio::test]
async fn test_system_time() {
    let app = make_app();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/system/time")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json["synced"].is_boolean());
    assert!(json["time"].is_string());
    // Verify time is RFC3339-parseable
    let time_str = json["time"].as_str().unwrap();
    chrono::DateTime::parse_from_rfc3339(time_str).expect("time should be RFC3339");
}

#[tokio::test]
async fn test_audio_source_is_unavailable_without_audio_player() {
    let app = app(AppState::with_run_dir("/nonexistent/kds"));
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/audio/source")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
}

// The volume and its limit are tested in volume_test.rs, against a stand-in
// Audio Player, which owns them (DEC-040).
