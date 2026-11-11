//! Tokens and the spoken setup code (AUTH_REQ_001-003, DEC-038).

use api_server::app;
use api_server::auth::TokenStore;
use api_server::setup::{
    random_code, spoken, CheckError, Setup, StartError, CODE_LIFETIME, MAX_ATTEMPTS,
};
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
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tower::ServiceExt;

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("kds-auth-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

// ---- The token store ----

#[test]
fn tokens_are_shown_once_and_stored_as_hashes() {
    let path = temp_dir("store").join("api/tokens.json");
    let store = TokenStore::load(&path);
    assert!(!store.has_admin());
    let (info, secret) = store.create("Tue's phone", true).unwrap();
    assert!(secret.starts_with("kds_") && secret.len() == 68, "{secret}");
    assert!(info.admin && store.has_admin());
    let file = std::fs::read_to_string(&path).unwrap();
    assert!(!file.contains(&secret), "the token itself is never stored");
    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );

    assert_eq!(store.verify(&secret).unwrap().id, info.id);
    assert!(store.verify("kds_wrong").is_none());
    assert!(store.list()[0].last_used.is_some(), "use is recorded");
    // Reloading keeps the tokens.
    assert!(TokenStore::load(&path).verify(&secret).is_some());
}

#[test]
fn the_last_admin_token_cannot_be_revoked() {
    let store = TokenStore::load(temp_dir("revoke").join("tokens.json"));
    let (admin, _) = store.create("admin", true).unwrap();
    let (kid, kid_secret) = store.create("tablet", false).unwrap();
    assert!(store.revoke(&admin.id).unwrap_err().contains("last admin"));
    store.revoke(&kid.id).unwrap();
    assert!(store.verify(&kid_secret).is_none());
    assert!(store.revoke("nope").unwrap_err().starts_with("no token"));
    assert!(store.create("  ", false).is_err());
}

#[test]
fn a_damaged_token_file_is_moved_aside() {
    let dir = temp_dir("damaged");
    std::fs::write(dir.join("tokens.json"), b"{not json").unwrap();
    let store = TokenStore::load(dir.join("tokens.json"));
    assert!(!store.has_admin());
    assert_eq!(
        std::fs::read(dir.join("tokens.json.damaged")).unwrap(),
        b"{not json"
    );
}

// ---- The setup code ----

#[test]
fn codes_are_six_digits_read_twice_in_danish() {
    let code = random_code();
    assert!(
        code.len() == 6 && code.chars().all(|c| c.is_ascii_digit()),
        "{code}"
    );
    let text = spoken("471293");
    assert_eq!(
        text,
        "Opsætningskode: 4, 7, 1, 2, 9, 3. Jeg gentager: 4, 7, 1, 2, 9, 3."
    );
}

#[test]
fn a_code_allows_five_attempts_and_is_used_once() {
    let setup = Setup::new();
    let t0 = Instant::now();
    assert_eq!(setup.check("000000", t0), Err(CheckError::NoCode));
    let code = setup.current_or_new(t0, false);
    let wrong = if code == "000000" { "111111" } else { "000000" };
    for left in (1..MAX_ATTEMPTS).rev() {
        assert_eq!(setup.check(wrong, t0), Err(CheckError::Wrong(left)));
    }
    assert_eq!(setup.check(wrong, t0), Err(CheckError::Wrong(0)));
    assert_eq!(
        setup.check(&code, t0),
        Err(CheckError::NoCode),
        "gone after the last attempt"
    );

    let code = setup.current_or_new(t0, false);
    assert_eq!(setup.check(&format!(" {code} "), t0), Ok(()));
    assert_eq!(
        setup.check(&code, t0),
        Err(CheckError::NoCode),
        "a code works once"
    );
}

#[test]
fn a_code_expires_unless_the_speaker_keeps_reading_it() {
    let setup = Setup::new();
    let t0 = Instant::now();
    let code = setup.current_or_new(t0, false);
    assert_eq!(
        setup.check(&code, t0 + CODE_LIFETIME),
        Err(CheckError::NoCode)
    );
    let code = setup.current_or_new(t0, true);
    let later = t0 + CODE_LIFETIME - Duration::from_secs(1);
    assert_eq!(
        setup.current_or_new(later, true),
        code,
        "reading it again keeps it"
    );
    assert_eq!(
        setup.check(&code, later + CODE_LIFETIME - Duration::from_secs(1)),
        Ok(())
    );
}

#[test]
fn a_new_code_can_be_requested_once_a_minute() {
    let setup = Setup::new();
    let t0 = Instant::now();
    let first = setup.request_new(t0).unwrap();
    assert!(matches!(
        setup.request_new(t0 + Duration::from_secs(10)),
        Err(StartError::TooSoon(_))
    ));
    let second = setup.request_new(t0 + Duration::from_secs(61)).unwrap();
    assert_eq!(
        setup.check(&first, t0 + Duration::from_secs(62)).is_ok(),
        first == second
    );
}

// ---- HTTP ----

fn from(ip: [u8; 4], transport: Transport, state: AppState) -> Router {
    app(state)
        .layer(Extension(transport))
        .layer(MockConnectInfo(SocketAddr::from((ip, 50000))))
}

const LAN: [u8; 4] = [192, 168, 1, 50];
const LOOPBACK: [u8; 4] = [127, 0, 0, 1];

async fn call(
    router: Router,
    method: &str,
    uri: &str,
    token: Option<&str>,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut request = Request::builder().method(method).uri(uri);
    if let Some(t) = token {
        request = request.header("authorization", format!("Bearer {t}"));
    }
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
async fn requests_from_the_network_need_a_token() {
    let state = AppState::with_run_dir(temp_dir("http-need"));
    let (_, secret) = state.tokens.create("tablet", false).unwrap();
    let (status, body) = call(
        from(LAN, Transport::Https, state.clone()),
        "GET",
        "/api/audio/source",
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["error"], "missing or invalid token");
    let (status, _) = call(
        from(LAN, Transport::Https, state.clone()),
        "GET",
        "/api/audio/source",
        Some("kds_bad"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    // With a token it reaches the route (no Audio Player runs here: 503).
    let (status, _) = call(
        from(LAN, Transport::Https, state.clone()),
        "GET",
        "/api/audio/source",
        Some(&secret),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    let (status, _) = call(
        from(LAN, Transport::Http, state.clone()),
        "GET",
        "/health",
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "/health stays open");
}

#[tokio::test]
async fn the_device_itself_needs_no_token_but_only_over_loopback_http() {
    let state = AppState::with_run_dir(temp_dir("http-local"));
    let (status, _) = call(
        from(LOOPBACK, Transport::Http, state.clone()),
        "GET",
        "/api/tokens",
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "local processes have full access");
    let (status, _) = call(
        from(LOOPBACK, Transport::Https, state.clone()),
        "GET",
        "/api/tokens",
        None,
        None,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "HTTPS is the network listener"
    );
    let (status, _) = call(
        from(LAN, Transport::Http, state.clone()),
        "GET",
        "/api/tokens",
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn managing_tokens_needs_an_admin_token_over_https() {
    let state = AppState::with_run_dir(temp_dir("http-admin"));
    let (_, admin) = state.tokens.create("parent", true).unwrap();
    let (_, kid) = state.tokens.create("tablet", false).unwrap();
    let (status, body) = call(
        from(LAN, Transport::Https, state.clone()),
        "GET",
        "/api/tokens",
        Some(&kid),
        None,
    )
    .await;
    assert_eq!(
        (status, body["error"].clone()),
        (StatusCode::FORBIDDEN, json!("an admin token is required"))
    );
    let (status, body) = call(
        from(LAN, Transport::Http, state.clone()),
        "GET",
        "/api/tokens",
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(
        (status, body["error"].clone()),
        (StatusCode::FORBIDDEN, json!("this endpoint requires HTTPS"))
    );

    let https = || from(LAN, Transport::Https, state.clone());
    let (status, body) = call(
        https(),
        "POST",
        "/api/tokens",
        Some(&admin),
        Some(json!({"name": "Home Assistant"})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let new_token = body["token"].as_str().unwrap().to_string();
    let new_id = body["info"]["id"].as_str().unwrap().to_string();
    assert_eq!(body["info"]["admin"], false);
    let (status, body) = call(https(), "GET", "/api/tokens", Some(&admin), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["tokens"].as_array().unwrap().len(), 3);
    assert!(
        !body.to_string().contains("kds_"),
        "listing never shows tokens"
    );
    let (status, _) = call(
        https(),
        "DELETE",
        &format!("/api/tokens/{new_id}"),
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = call(https(), "GET", "/api/audio/source", Some(&new_token), None).await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "a revoked token no longer works"
    );
}

/// A stand-in Audio Player that records what it is asked to announce.
async fn audio_player(run: &Path) -> Arc<Mutex<Vec<String>>> {
    let said: Arc<Mutex<Vec<String>>> = Arc::default();
    let path = run.join("audio.sock");
    let server_path = path.clone();
    let log = Arc::clone(&said);
    tokio::spawn(async move {
        kds_ipc::serve(server_path, move |request: Value| {
            let log = Arc::clone(&log);
            async move {
                match command(&request) {
                    Some("announce") => {
                        log.lock()
                            .unwrap()
                            .push(request["text"].as_str().unwrap().to_string());
                        ok(json!({"position": 1}))
                    }
                    Some("status") => ok(json!({"announcing": false})),
                    _ => error("unknown command"),
                }
            }
        })
        .await
        .unwrap();
    });
    for _ in 0..100 {
        if tokio::net::UnixStream::connect(&path).await.is_ok() {
            return said;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("stand-in audio player did not start");
}

fn code_in(text: &str) -> String {
    text.chars().filter(char::is_ascii_digit).take(6).collect()
}

#[tokio::test]
async fn setup_exchanges_the_spoken_code_for_an_admin_token() {
    let run = temp_dir("http-setup");
    let said = audio_player(&run).await;
    let state = AppState::with_run_dir(&run);
    let net = || from(LAN, Transport::Https, state.clone());

    let (status, _) = call(
        net(),
        "POST",
        "/api/setup",
        None,
        Some(json!({"code": "123456", "name": "x"})),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "no code has been read yet");
    let (status, body) = call(net(), "POST", "/api/setup/start", None, None).await;
    assert_eq!(status, StatusCode::ACCEPTED, "{body}");
    let (status, body) = call(net(), "POST", "/api/setup/start", None, None).await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    assert!(body["retry_after_s"].as_u64().unwrap() > 50);

    let code = code_in(&said.lock().unwrap()[0]);
    let wrong = if code == "000000" { "111111" } else { "000000" };
    let (status, body) = call(
        net(),
        "POST",
        "/api/setup",
        None,
        Some(json!({"code": wrong, "name": "x"})),
    )
    .await;
    assert_eq!(
        (status, body["attempts_left"].clone()),
        (StatusCode::FORBIDDEN, json!(4))
    );
    let (status, body) = call(
        net(),
        "POST",
        "/api/setup",
        None,
        Some(json!({"code": code, "name": "Tue's phone"})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["info"]["admin"], true);
    let token = body["token"].as_str().unwrap().to_string();
    let (status, _) = call(net(), "GET", "/api/tokens", Some(&token), None).await;
    assert_eq!(status, StatusCode::OK, "the new admin token works");
}

#[tokio::test]
async fn an_unconfigured_speaker_keeps_reading_the_code_until_set_up() {
    let run = temp_dir("http-repeat");
    let said = audio_player(&run).await;
    let state = AppState::with_run_dir(&run);
    let reader = tokio::spawn(api_server::routes::setup::read_until_set_up_every(
        state.clone(),
        Duration::from_millis(50),
    ));
    tokio::time::sleep(Duration::from_millis(1500)).await;
    let readings = said.lock().unwrap().clone();
    assert!(readings.len() >= 2, "repeated: {readings:?}");
    assert!(
        readings.iter().all(|r| code_in(r) == code_in(&readings[0])),
        "the same code each time"
    );

    // Using the code sets the speaker up; the reading stops.
    let net = from(LAN, Transport::Https, state.clone());
    let (status, _) = call(
        net,
        "POST",
        "/api/setup",
        None,
        Some(json!({"code": code_in(&readings[0]), "name": "parent"})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    tokio::time::timeout(Duration::from_secs(5), reader)
        .await
        .expect("the reading stops")
        .unwrap();
}
