use anyhow::{Context, Result};
use api_server::tls::{self, TLS_DIR};
use api_server::transport::Transport;
use api_server::{app, state::AppState};
use axum::Extension;
use axum_server::tls_rustls::RustlsConfig;

/// HTTPS for the Web UI and API (WEB_REQ_001); plain HTTP stays for health
/// checks and tests.
const HTTPS_ADDR: &str = "0.0.0.0:443";
const HTTP_ADDR: &str = "0.0.0.0:8080";

#[tokio::main]
async fn main() -> Result<()> {
    rustls::crypto::ring::default_provider()
        .install_default()
        .map_err(|_| anyhow::anyhow!("could not install the TLS crypto provider"))?;

    let state = AppState::new();

    // Unnamed voice messages expire 90 days after they were last played
    // (MSG_REQ_003): checked at start and then daily.
    let messages = std::sync::Arc::clone(&state.messages);
    tokio::spawn(async move {
        loop {
            let removed = messages.evict(std::time::SystemTime::now());
            if removed > 0 {
                println!("api-server: removed {removed} expired voice messages");
            }
            tokio::time::sleep(std::time::Duration::from_secs(24 * 3600)).await;
        }
    });
    let http = app(state.clone()).layer(Extension(Transport::Http));
    let https = app(state).layer(Extension(Transport::Https));

    // Without a certificate the speaker still answers on plain HTTP, so it can
    // be reached and diagnosed.
    let hostname = std::fs::read_to_string("/proc/sys/kernel/hostname").unwrap_or_default();
    match tls::load_or_create(TLS_DIR.as_ref(), &hostname) {
        Ok(cert) => {
            if cert.created {
                println!(
                    "api-server: created a certificate for {}",
                    tls::names_for(&hostname)[0]
                );
            }
            let config =
                RustlsConfig::from_pem(cert.cert_pem.into_bytes(), cert.key_pem.into_bytes())
                    .await
                    .context("loading the certificate")?;
            let addr = HTTPS_ADDR.parse()?;
            tokio::spawn(async move {
                if let Err(e) = axum_server::bind_rustls(addr, config)
                    .serve(https.into_make_service())
                    .await
                {
                    eprintln!("api-server: HTTPS stopped: {e}");
                }
            });
            println!("api-server: HTTPS on {HTTPS_ADDR}");
        }
        Err(e) => eprintln!("api-server: no certificate, HTTPS disabled: {e:#}"),
    }

    let listener = tokio::net::TcpListener::bind(HTTP_ADDR).await?;
    println!("api-server: HTTP on {HTTP_ADDR}");
    axum::serve(listener, http).await?;
    Ok(())
}
