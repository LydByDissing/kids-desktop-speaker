use ota_updater::installer::{download_verified, download_with_read_timeout, sha256_hex};
use ota_updater::release::fetch_latest;
use ota_updater::{InstallerError, OtaConfig, ReleaseError};
use serde_json::json;
use std::path::PathBuf;
use std::time::Duration;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const BUNDLE: &[u8] = b"signed rauc bundle";

fn config(server: &MockServer) -> OtaConfig {
    OtaConfig {
        repo_owner: "owner".into(),
        repo_name: "repo".into(),
        check_interval: Duration::from_secs(1),
        version_file: "/nonexistent".into(),
        api_base: server.uri(),
        ..OtaConfig::from_env()
    }
}

fn temp_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("kds-{}-{name}", std::process::id()))
}

async fn serve(server: &MockServer, url: &str, response: ResponseTemplate) {
    Mock::given(method("GET"))
        .and(path(url.to_string()))
        .respond_with(response)
        .mount(server)
        .await;
}

async fn serve_release(server: &MockServer, assets: serde_json::Value) {
    serve(
        server,
        "/repos/owner/repo/releases/latest",
        ResponseTemplate::new(200).set_body_json(json!({"tag_name": "v1.2.3", "assets": assets})),
    )
    .await;
}

#[tokio::test]
async fn fetch_latest_resolves_manifest_and_bundle_url() {
    let server = MockServer::start().await;
    let base = server.uri();
    serve_release(
        &server,
        json!([
            {"name": "manifest.json", "browser_download_url": format!("{base}/manifest.json")},
            {"name": "image.raucb", "browser_download_url": format!("{base}/image.raucb")}
        ]),
    )
    .await;
    serve(
        &server,
        "/manifest.json",
        ResponseTemplate::new(200).set_body_json(json!({
            "version": "v1.2.3", "bundle": "image.raucb", "sha256": "abc", "size": 18
        })),
    )
    .await;

    let update = fetch_latest(&config(&server)).await.unwrap().unwrap();
    assert_eq!(update.manifest.version, "v1.2.3");
    assert_eq!(update.bundle_url, format!("{base}/image.raucb"));
}

#[tokio::test]
async fn fetch_latest_errors_without_manifest_asset() {
    let server = MockServer::start().await;
    serve_release(&server, json!([])).await;

    let result = fetch_latest(&config(&server)).await;
    assert!(matches!(result, Err(ReleaseError::AssetNotFound(_))));
}

#[tokio::test]
async fn no_release_yet_is_a_normal_answer() {
    let server = MockServer::start().await;
    serve(
        &server,
        "/repos/owner/repo/releases/latest",
        ResponseTemplate::new(404),
    )
    .await;

    // GitHub's answer when the repository has only pre-releases (KDS-eio).
    let result = fetch_latest(&config(&server)).await;
    assert!(matches!(result, Ok(None)));
}

#[tokio::test]
async fn other_http_errors_still_fail_the_check() {
    let server = MockServer::start().await;
    serve(
        &server,
        "/repos/owner/repo/releases/latest",
        ResponseTemplate::new(403),
    )
    .await;

    let result = fetch_latest(&config(&server)).await;
    assert!(matches!(result, Err(ReleaseError::Http(_))), "rate limited");
}

#[tokio::test]
async fn download_writes_a_bundle_with_matching_checksum() {
    let server = MockServer::start().await;
    serve(
        &server,
        "/image.raucb",
        ResponseTemplate::new(200).set_body_bytes(BUNDLE),
    )
    .await;
    let target = temp_path("good");

    let url = format!("{}/image.raucb", server.uri());
    download_verified(
        &url,
        &sha256_hex(BUNDLE),
        Some(BUNDLE.len() as u64),
        &target,
    )
    .await
    .unwrap();

    assert_eq!(std::fs::read(&target).unwrap(), BUNDLE);
    std::fs::remove_file(&target).unwrap();
}

#[tokio::test]
async fn download_removes_a_bundle_with_wrong_checksum() {
    let server = MockServer::start().await;
    serve(
        &server,
        "/image.raucb",
        ResponseTemplate::new(200).set_body_bytes(BUNDLE),
    )
    .await;
    let target = temp_path("tampered");

    let url = format!("{}/image.raucb", server.uri());
    let result = download_verified(
        &url,
        &sha256_hex(b"something else"),
        Some(BUNDLE.len() as u64),
        &target,
    )
    .await;

    assert!(matches!(
        result,
        Err(InstallerError::ChecksumMismatch { .. })
    ));
    assert!(!target.exists());
}

#[tokio::test]
async fn download_reports_http_errors_and_leaves_no_file() {
    let server = MockServer::start().await;
    serve(&server, "/image.raucb", ResponseTemplate::new(500)).await;
    let target = temp_path("server-error");

    let url = format!("{}/image.raucb", server.uri());
    let result = download_verified(&url, "abc", None, &target).await;

    assert!(matches!(result, Err(InstallerError::Download(_))));
    assert!(!target.exists());
}

#[tokio::test]
async fn download_fails_when_the_connection_stalls() {
    let server = MockServer::start().await;
    serve(
        &server,
        "/image.raucb",
        ResponseTemplate::new(200)
            .set_body_bytes(BUNDLE)
            .set_delay(Duration::from_secs(5)),
    )
    .await;
    let target = temp_path("stalled");

    let url = format!("{}/image.raucb", server.uri());
    let started = std::time::Instant::now();
    let result = download_with_read_timeout(
        &url,
        &sha256_hex(BUNDLE),
        None,
        &target,
        Duration::from_millis(200),
    )
    .await;

    assert!(matches!(result, Err(InstallerError::Download(_))));
    assert!(started.elapsed() < Duration::from_secs(4));
    assert!(!target.exists());
}

#[tokio::test]
async fn short_download_is_incomplete_not_a_bad_release() {
    let server = MockServer::start().await;
    serve(
        &server,
        "/image.raucb",
        ResponseTemplate::new(200).set_body_bytes(&BUNDLE[..5]),
    )
    .await;
    let target = temp_path("short");

    let url = format!("{}/image.raucb", server.uri());
    let result = download_verified(
        &url,
        &sha256_hex(BUNDLE),
        Some(BUNDLE.len() as u64),
        &target,
    )
    .await;

    let err = result.unwrap_err();
    assert!(matches!(err, InstallerError::Incomplete { .. }));
    assert!(!err.is_bad_release());
    assert!(!target.exists());
}
