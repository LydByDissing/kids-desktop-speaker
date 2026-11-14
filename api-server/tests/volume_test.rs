//! The volume API against a stand-in Audio Player (VOL_REQ_001, DEC-040).

use api_server::app;
use api_server::state::AppState;
use api_server::transport::Transport;
use axum::body::Body;
use axum::extract::connect_info::MockConnectInfo;
use axum::http::{Request, StatusCode};
use axum::{Extension, Router};
use http_body_util::BodyExt;
use kds_ipc::{command, error, ok};
use serde_json::{json, Value};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tower::ServiceExt;

fn run_dir(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("kds-api-vol-{}-{name}", std::process::id()))
}

/// A stand-in Audio Player holding a volume and a limit, as the real one does.
async fn audio_player(dir: &Path) -> Arc<Mutex<(u64, u64)>> {
    let state = Arc::new(Mutex::new((40u64, 75u64)));
    let path = dir.join("audio.sock");
    let server_path = path.clone();
    let st = Arc::clone(&state);
    tokio::spawn(async move {
        kds_ipc::serve(server_path, move |request: Value| {
            let st = Arc::clone(&st);
            async move {
                let mut s = st.lock().unwrap();
                match command(&request) {
                    Some("set_volume") => match request["volume"].as_u64() {
                        Some(v) if v <= 100 => s.0 = v.min(s.1),
                        _ => return error("volume must be a whole number from 0 to 100"),
                    },
                    Some("set_limit") => match request["limit"].as_u64() {
                        Some(l) if l <= 100 => {
                            s.1 = l;
                            s.0 = s.0.min(l);
                        }
                        _ => return error("limit must be a whole number from 0 to 100"),
                    },
                    Some("volume") => {}
                    // The bass fill (DEC-053): the stand-in echoes a curve.
                    Some("bass_fill") => return ok(json!({"max_db": 6.0, "top_db": 6.0})),
                    Some("set_bass_fill") => {
                        return ok(
                            json!({"max_db": request["max_db"], "top_db": request["top_db"]}),
                        )
                    }
                    _ => return error("unknown command"),
                }
                ok(json!({"volume": s.0, "limit": s.1}))
            }
        })
        .await
        .unwrap();
    });
    for _ in 0..100 {
        if tokio::net::UnixStream::connect(&path).await.is_ok() {
            return state;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("stand-in audio player did not start");
}

fn net(state: AppState, transport: Transport) -> Router {
    app(state)
        .layer(Extension(transport))
        .layer(MockConnectInfo(SocketAddr::from(([192, 168, 1, 50], 5000))))
}

async fn call(
    router: Router,
    method: &str,
    uri: &str,
    token: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut request = Request::builder()
        .method(method)
        .uri(uri)
        .header("authorization", format!("Bearer {token}"));
    let body = match body {
        Some(b) => {
            request = request.header("content-type", "application/json");
            Body::from(b.to_string())
        }
        None => Body::empty(),
    };
    let response = router.oneshot(request.body(body).unwrap()).await.unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

#[tokio::test]
async fn anyone_with_a_token_sets_the_volume_up_to_the_limit() {
    let dir = run_dir("volume");
    audio_player(&dir).await;
    let state = AppState::with_run_dir(&dir);
    let (_, kid) = state.tokens.create("tablet", false).unwrap();
    let (status, body) = call(
        net(state.clone(), Transport::Https),
        "GET",
        "/api/audio/volume",
        &kid,
        None,
    )
    .await;
    assert_eq!(
        (status, body["volume"].clone()),
        (StatusCode::OK, json!(40))
    );
    let (status, body) = call(
        net(state.clone(), Transport::Http),
        "POST",
        "/api/audio/volume",
        &kid,
        Some(json!({"volume": 90})),
    )
    .await;
    assert_eq!(
        (status, body["volume"].clone()),
        (StatusCode::OK, json!(75)),
        "capped at the limit"
    );
    let (status, _) = call(
        net(state.clone(), Transport::Https),
        "POST",
        "/api/audio/volume",
        &kid,
        Some(json!({"volume": 150})),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn only_an_admin_over_https_changes_the_limit() {
    let dir = run_dir("limit");
    let player = audio_player(&dir).await;
    let state = AppState::with_run_dir(&dir);
    let (_, kid) = state.tokens.create("tablet", false).unwrap();
    let (_, parent) = state.tokens.create("parent", true).unwrap();
    let limit = |v: u64| Some(json!({"limit": v}));

    let (status, body) = call(
        net(state.clone(), Transport::Https),
        "GET",
        "/api/audio/volume-limit",
        &kid,
        None,
    )
    .await;
    assert_eq!(
        (status, body["limit"].clone()),
        (StatusCode::OK, json!(75)),
        "anyone may read it"
    );
    let (status, _) = call(
        net(state.clone(), Transport::Https),
        "POST",
        "/api/audio/volume-limit",
        &kid,
        limit(100),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "a child's token can't raise it"
    );
    let (status, _) = call(
        net(state.clone(), Transport::Http),
        "POST",
        "/api/audio/volume-limit",
        &parent,
        limit(50),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "not over plain HTTP");
    assert_eq!(*player.lock().unwrap(), (40, 75), "nothing changed");
    let (status, body) = call(
        net(state.clone(), Transport::Https),
        "POST",
        "/api/audio/volume-limit",
        &parent,
        limit(30),
    )
    .await;
    assert_eq!(
        (status, body["limit"].clone(), body["volume"].clone()),
        (StatusCode::OK, json!(30), json!(30))
    );
    let (status, _) = call(
        net(state, Transport::Https),
        "POST",
        "/api/audio/volume-limit",
        &parent,
        limit(101),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn only_an_admin_over_https_changes_the_bass_fill() {
    let dir = run_dir("bass");
    audio_player(&dir).await;
    let state = AppState::with_run_dir(&dir);
    let (_, kid) = state.tokens.create("tablet", false).unwrap();
    let (_, parent) = state.tokens.create("parent", true).unwrap();
    let curve = || Some(json!({"max_db": 18.0, "top_db": 6.0}));

    let (status, body) = call(
        net(state.clone(), Transport::Https),
        "GET",
        "/api/audio/bass",
        &kid,
        None,
    )
    .await;
    assert_eq!(
        (status, body["top_db"].clone()),
        (StatusCode::OK, json!(6.0)),
        "anyone may read it"
    );
    let (status, _) = call(
        net(state.clone(), Transport::Https),
        "POST",
        "/api/audio/bass",
        &kid,
        curve(),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "a child's token can't change it"
    );
    let (status, _) = call(
        net(state.clone(), Transport::Http),
        "POST",
        "/api/audio/bass",
        &parent,
        curve(),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "not over plain HTTP");
    let (status, body) = call(
        net(state, Transport::Https),
        "POST",
        "/api/audio/bass",
        &parent,
        curve(),
    )
    .await;
    assert_eq!(
        (status, body["max_db"].clone()),
        (StatusCode::OK, json!(18.0))
    );
}
