use api_server::app;
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

#[tokio::test]
async fn test_get_volume_limit() {
    let app = make_app();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/audio/volume-limit")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    // Default volume limit is 75
    assert_eq!(json["limit"], 75);
}

#[tokio::test]
async fn test_set_volume_limit() {
    let app = make_app();

    // Valid set
    let body = serde_json::json!({ "limit": 50 }).to_string();
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/audio/volume-limit")
                .header("content-type", "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let resp_body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&resp_body).unwrap();
    assert_eq!(json["limit"], 50);

    // Invalid set (out of range — u8 can't exceed 100 without being > 100, test 101 isn't possible
    // with u8. Test that the validation triggers on a specially crafted raw JSON with value > 100.
    // Since u8 max is 255, we test value 101.)
    let body_invalid = r#"{"limit": 101}"#;
    let response_invalid = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/audio/volume-limit")
                .header("content-type", "application/json")
                .body(Body::from(body_invalid))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response_invalid.status(), StatusCode::UNPROCESSABLE_ENTITY);
}
