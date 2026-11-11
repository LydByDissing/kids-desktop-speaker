//! HTTPS (WEB_REQ_001): the device certificate and the guard for secret-bearing
//! routes, including a real TLS connection to the API.

use api_server::tls::{load_or_create, names_for};
use api_server::transport::{require_https, Transport};
use api_server::{app, state::AppState};
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::{middleware, routing::post, Extension, Router};
use axum_server::tls_rustls::RustlsConfig;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tower::ServiceExt;

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("kds-tls-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir.join("tls")
}

fn mode(path: &std::path::Path) -> u32 {
    std::fs::metadata(path).unwrap().permissions().mode() & 0o777
}

#[test]
fn certificate_names_the_device() {
    assert_eq!(
        names_for("kidsspeaker-2df5\n"),
        vec!["kidsspeaker-2df5.local", "kidsspeaker-2df5", "localhost"]
    );
    assert_eq!(names_for(""), vec!["localhost"]);
}

#[test]
fn a_new_certificate_is_created_with_a_private_key() {
    let dir = temp_dir("create");
    let cert = load_or_create(&dir, "kidsspeaker-test").unwrap();
    assert!(cert.created);
    assert!(cert.cert_pem.starts_with("-----BEGIN CERTIFICATE-----"));
    assert!(cert.key_pem.contains("PRIVATE KEY"));
    assert_eq!(mode(&dir.join("key.pem")), 0o600, "the key is private");
    assert_eq!(mode(&dir), 0o700);
    assert!(!dir.join("key.tmp").exists() && !dir.join("cert.tmp").exists());
}

#[test]
fn the_certificate_is_reused_until_the_hostname_changes() {
    let dir = temp_dir("reuse");
    let first = load_or_create(&dir, "kidsspeaker-test").unwrap();
    let again = load_or_create(&dir, "kidsspeaker-test").unwrap();
    assert!(!again.created, "a restart or update keeps the certificate");
    assert_eq!(again.cert_pem, first.cert_pem);
    let renamed = load_or_create(&dir, "kidsspeaker-beef").unwrap();
    assert!(renamed.created, "a new hostname gets a new certificate");
    assert_ne!(renamed.cert_pem, first.cert_pem);
}

/// GET /health over TLS, trusting only the device's own certificate, with
/// `server_name` as the name the client asks for. Returns the status line.
async fn https_get_health(cert_pem: &str, port: u16, server_name: &str) -> String {
    let mut roots = rustls::RootCertStore::empty();
    let mut reader = std::io::BufReader::new(cert_pem.as_bytes());
    for cert in rustls_pemfile_certs(&mut reader) {
        roots.add(cert).unwrap();
    }
    let config = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .unwrap()
    .with_root_certificates(roots)
    .with_no_client_auth();
    let connector = tokio_rustls::TlsConnector::from(Arc::new(config));
    let tcp = tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .unwrap();
    let name = rustls::pki_types::ServerName::try_from(server_name.to_string()).unwrap();
    let mut tls = connector.connect(name, tcp).await.expect("TLS handshake");
    tls.write_all(
        format!("GET /health HTTP/1.1\r\nHost: {server_name}\r\nConnection: close\r\n\r\n")
            .as_bytes(),
    )
    .await
    .unwrap();
    let mut response = String::new();
    tls.read_to_string(&mut response).await.unwrap();
    response.lines().next().unwrap_or_default().to_string()
}

/// Parses PEM certificates without another dependency.
fn rustls_pemfile_certs(
    reader: &mut dyn std::io::BufRead,
) -> Vec<rustls::pki_types::CertificateDer<'static>> {
    use rustls::pki_types::pem::PemObject;
    let mut pem = String::new();
    reader.read_to_string(&mut pem).unwrap();
    rustls::pki_types::CertificateDer::pem_slice_iter(pem.as_bytes())
        .map(|c| c.unwrap())
        .collect()
}

#[tokio::test]
async fn the_api_answers_over_https_with_the_device_certificate() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let cert = load_or_create(&temp_dir("serve"), "kidsspeaker-test").unwrap();
    let config = RustlsConfig::from_pem(
        cert.cert_pem.clone().into_bytes(),
        cert.key_pem.into_bytes(),
    )
    .await
    .unwrap();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let router =
        app(AppState::with_run_dir(temp_dir("serve-run"))).layer(Extension(Transport::Https));
    tokio::spawn(async move {
        axum_server::from_tcp_rustls(listener, config)
            .serve(router.into_make_service())
            .await
            .unwrap();
    });
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    for name in ["localhost", "kidsspeaker-test.local"] {
        let status = https_get_health(&cert.cert_pem, port, name).await;
        assert!(status.starts_with("HTTP/1.1 200"), "{name}: {status}");
    }
}

fn guarded() -> Router {
    Router::new()
        .route("/secret", post(|| async { "stored" }))
        .route_layer(middleware::from_fn(require_https))
}

async fn post_secret(router: Router) -> StatusCode {
    router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/secret")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap()
        .status()
}

#[tokio::test]
async fn secret_routes_reject_plain_http() {
    assert_eq!(
        post_secret(guarded().layer(Extension(Transport::Http))).await,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        post_secret(guarded()).await,
        StatusCode::FORBIDDEN,
        "no marker counts as plain HTTP"
    );
    assert_eq!(
        post_secret(guarded().layer(Extension(Transport::Https))).await,
        StatusCode::OK
    );
}
