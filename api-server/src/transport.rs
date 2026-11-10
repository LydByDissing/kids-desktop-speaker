//! Which listener a request arrived on, and the guard for routes that carry
//! secrets (WEB_REQ_001): they answer 403 over plain HTTP.

use axum::{
    extract::Request,
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
    Extension, Json,
};
use serde_json::json;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transport {
    Https,
    Http,
}

/// Rejects a request unless it came over HTTPS. A request with no transport
/// marker is treated as plain HTTP, so a missing layer fails safe.
pub async fn require_https(
    transport: Option<Extension<Transport>>,
    request: Request,
    next: Next,
) -> Response {
    match transport {
        Some(Extension(Transport::Https)) => next.run(request).await,
        _ => (
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "this endpoint requires HTTPS" })),
        )
            .into_response(),
    }
}
