//! The Web UI's static files (DEC-045).

use api_server::app;
use api_server::state::AppState;
use api_server::transport::Transport;
use api_server::web::{fallback, CONTENT_SECURITY_POLICY};
use axum::body::Body;
use axum::extract::connect_info::MockConnectInfo;
use axum::http::{header, Request, StatusCode};
use axum::{Extension, Router};
use http_body_util::BodyExt;
use std::net::SocketAddr;
use std::path::PathBuf;
use tower::ServiceExt;

fn web_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("kds-web-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("assets")).unwrap();
    std::fs::write(dir.join("index.html"), "<!doctype html><div id=app></div>").unwrap();
    std::fs::write(dir.join("assets/index-abc.js"), "console.log(1)").unwrap();
    dir
}

async fn get(router: Router, uri: &str) -> (StatusCode, axum::http::HeaderMap, String) {
    let response = router
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let body = String::from_utf8(
        response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec(),
    )
    .unwrap();
    (status, headers, body)
}

#[tokio::test]
async fn the_page_and_its_files_are_served_with_security_headers() {
    let dir = web_dir("files");
    let (status, headers, body) = get(fallback(dir.clone()), "/").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("id=app"));
    assert_eq!(
        headers[header::CONTENT_SECURITY_POLICY],
        CONTENT_SECURITY_POLICY
    );
    assert_eq!(headers[header::X_CONTENT_TYPE_OPTIONS], "nosniff");
    assert_eq!(headers[header::REFERRER_POLICY], "no-referrer");
    let (status, headers, body) = get(fallback(dir), "/assets/index-abc.js").await;
    assert_eq!((status, body.as_str()), (StatusCode::OK, "console.log(1)"));
    assert!(headers[header::CONTENT_TYPE]
        .to_str()
        .unwrap()
        .contains("javascript"));
}

#[tokio::test]
async fn the_apps_own_paths_get_the_page_but_unknown_api_paths_a_404() {
    let dir = web_dir("fallback");
    let (status, _, body) = get(fallback(dir.clone()), "/settings/access").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("id=app"), "the app handles its own paths");
    let (status, _, body) = get(fallback(dir), "/api/nothing-here").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body, r#"{"error":"not found"}"#);
}

#[tokio::test]
async fn the_page_needs_no_token_but_the_api_does() {
    let dir = web_dir("app");
    std::env::set_var("KDS_WEB_DIR", &dir);
    let run = std::env::temp_dir().join(format!("kds-web-run-{}", std::process::id()));
    let router = || {
        app(AppState::with_run_dir(&run))
            .layer(Extension(Transport::Https))
            .layer(MockConnectInfo(SocketAddr::from(([192, 168, 1, 50], 5000))))
    };
    let (status, _, body) = get(router(), "/").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, _, _) = get(router(), "/api/audio/source").await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}
