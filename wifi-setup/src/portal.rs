//! The captive portal (NET_ONB_001, DEC-044): a page that asks for the home
//! network, served at http://192.168.4.1 while the setup access point is up.
//! Every path answers with the page, so phones' captive-portal checks open it.

use axum::{
    extract::State,
    response::{Html, IntoResponse},
    routing::{any, post},
    Form, Router,
};
use serde::Deserialize;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

use crate::wpa;

#[derive(Clone)]
pub struct Portal {
    /// The boot partition's wifi.conf (setup-sd.sh writes the same file).
    pub boot_conf: PathBuf,
    /// Where wpa_supplicant reads it from now.
    pub live_conf: PathBuf,
    /// Networks seen before the access point opened, for the list.
    pub networks: Arc<Mutex<Vec<String>>>,
    /// Told the network name once a network is saved.
    pub saved: mpsc::Sender<String>,
    /// The speaker's name, for the page title.
    pub name: String,
}

pub fn escape(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            '&' => "&amp;".to_string(),
            '<' => "&lt;".to_string(),
            '>' => "&gt;".to_string(),
            '"' => "&quot;".to_string(),
            '\'' => "&#39;".to_string(),
            c => c.to_string(),
        })
        .collect()
}

fn page(title: &str, body: &str) -> Html<String> {
    Html(format!(
        r#"<!doctype html><html lang="da"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title}</title>
<style>body{{font-family:system-ui,sans-serif;max-width:28rem;margin:2rem auto;padding:0 1rem;color:#222}}
label{{display:block;margin-top:1rem;font-weight:600}}input,select,button{{width:100%;font-size:1.1rem;padding:.6rem;margin-top:.3rem;box-sizing:border-box}}
button{{margin-top:1.5rem;background:#2563eb;color:#fff;border:0;border-radius:.4rem}}.err{{color:#b91c1c}}</style>
</head><body>{body}</body></html>"#
    ))
}

fn form(portal: &Portal, error: Option<&str>) -> Html<String> {
    let networks = portal
        .networks
        .lock()
        .map(|n| n.clone())
        .unwrap_or_default();
    let options: String = networks
        .iter()
        .map(|n| format!("<option value=\"{0}\">{0}</option>", escape(n)))
        .collect();
    let error = error
        .map(|e| format!("<p class=\"err\">{}</p>", escape(e)))
        .unwrap_or_default();
    page(
        &format!("Sæt {} op", escape(&portal.name)),
        &format!(
            r#"<h1>Wi-Fi til {name}</h1>
<p>Vælg dit hjemmenetværk og skriv adgangskoden. Højttaleren forbinder sig og
denne side lukker.</p>{error}
<form method="post" action="/setup">
<label for="ssid">Netværk</label>
<select id="ssid" name="ssid">{options}<option value="">Andet netværk …</option></select>
<label for="other">Andet netværk (navn)</label>
<input id="other" name="other" autocomplete="off">
<label for="password">Adgangskode</label>
<input id="password" name="password" type="password" autocomplete="off">
<button type="submit">Forbind</button>
</form>"#,
            name = escape(&portal.name)
        ),
    )
}

#[derive(Debug, Deserialize)]
pub struct SetupForm {
    #[serde(default)]
    pub ssid: String,
    #[serde(default)]
    pub other: String,
    #[serde(default)]
    pub password: String,
}

/// Writes `contents` to `path` atomically, readable by root only.
fn write_private(path: &std::path::Path, contents: &str) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let temp = path.with_extension("tmp");
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&temp)?;
    f.write_all(contents.as_bytes())?;
    f.sync_all()?;
    std::fs::rename(&temp, path)
}

async fn show(State(portal): State<Portal>) -> Html<String> {
    form(&portal, None)
}

async fn setup(State(portal): State<Portal>, Form(input): Form<SetupForm>) -> impl IntoResponse {
    let ssid = if input.other.trim().is_empty() {
        input.ssid
    } else {
        input.other.trim().to_string()
    };
    let config = match wpa::config(&ssid, &input.password) {
        Ok(c) => c,
        Err(e) => return form(&portal, Some(&e)),
    };
    if let Err(e) = write_private(&portal.boot_conf, &config)
        .and_then(|()| write_private(&portal.live_conf, &config))
    {
        return form(&portal, Some(&format!("Kunne ikke gemme netværket: {e}")));
    }
    let _ = portal.saved.send(ssid.clone()).await;
    page(
        "Forbinder",
        &format!(
            "<h1>Gemt</h1><p>{} forbinder nu til <b>{}</b>. Forbind din telefon til samme netværk \
             og åbn <b>https://{}.local</b>.</p>",
            escape(&portal.name),
            escape(&ssid),
            escape(&portal.name.to_lowercase())
        ),
    )
}

pub fn router(portal: Portal) -> Router {
    Router::new()
        .route("/setup", post(setup))
        .fallback(any(show))
        .with_state(portal)
}
