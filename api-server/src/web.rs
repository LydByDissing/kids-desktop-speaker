//! Serves the Web UI's static files at `/` (DEC-045). They need no token: the
//! page is public, the data behind it isn't. Paths the UI handles itself fall
//! back to index.html; unknown /api paths stay a JSON 404.

use axum::{
    body::Body,
    extract::Request,
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Json},
    Router,
};
use serde_json::json;
use std::path::PathBuf;
use tower::ServiceExt;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::set_header::SetResponseHeaderLayer;

pub const WEB_DIR: &str = "/usr/share/kds/web";

/// Only the speaker's own scripts run; inline styles are used for widths.
/// Microphone access needs HTTPS, which the speaker serves (WEB_REQ_001).
pub const CONTENT_SECURITY_POLICY: &str = "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; media-src 'self' blob:; connect-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'self'";

/// The fallback for everything the API doesn't route.
pub fn fallback(dir: PathBuf) -> Router {
    let index = dir.join("index.html");
    let files = ServeDir::new(&dir).fallback(ServeFile::new(index));
    Router::new()
        .fallback(move |request: Request| {
            let files = files.clone();
            async move {
                if request.uri().path().starts_with("/api/") {
                    return (StatusCode::NOT_FOUND, Json(json!({ "error": "not found" })))
                        .into_response();
                }
                match files.oneshot(request).await {
                    Ok(response) => response.map(Body::new),
                    Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
                }
            }
        })
        .layer(SetResponseHeaderLayer::overriding(
            header::CONTENT_SECURITY_POLICY,
            HeaderValue::from_static(CONTENT_SECURITY_POLICY),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            header::X_CONTENT_TYPE_OPTIONS,
            HeaderValue::from_static("nosniff"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            header::REFERRER_POLICY,
            HeaderValue::from_static("no-referrer"),
        ))
}
