//! API tokens (AUTH_REQ_001, AUTH_REQ_002, DEC-038).
//!
//! Every request needs a valid token, except `/health`, the setup endpoints and
//! requests from the device itself over loopback. Tokens are random, shown once
//! and stored only as SHA-256 hashes.

use axum::{
    extract::{ConnectInfo, Request, State},
    http::{header, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Extension, Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::state::AppState;
use crate::transport::Transport;

pub const TOKENS_FILE: &str = "/data/api/tokens.json";
/// "Last used" is saved at most this often, to spare the SD card.
const LAST_USED_RESOLUTION: u64 = 3600;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TokenRecord {
    id: String,
    name: String,
    hash: String,
    admin: bool,
    created: u64,
    last_used: Option<u64>,
}

/// What may be shown about a token: never the token or its hash.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TokenInfo {
    pub id: String,
    pub name: String,
    pub admin: bool,
    pub created: u64,
    pub last_used: Option<u64>,
}

impl From<&TokenRecord> for TokenInfo {
    fn from(r: &TokenRecord) -> Self {
        TokenInfo {
            id: r.id.clone(),
            name: r.name.clone(),
            admin: r.admin,
            created: r.created,
            last_used: r.last_used,
        }
    }
}

/// Who made a request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Principal {
    /// A process on the device itself, over loopback: full access.
    Local,
    Token(TokenInfo),
}

impl Principal {
    pub fn is_admin(&self) -> bool {
        match self {
            Principal::Local => true,
            Principal::Token(t) => t.admin,
        }
    }
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn random_hex(bytes: usize) -> String {
    use std::io::Read;
    let mut buf = vec![0u8; bytes];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut buf))
        .expect("the system's random source must be readable");
    buf.iter().map(|b| format!("{b:02x}")).collect()
}

fn hash(secret: &str) -> String {
    Sha256::digest(secret.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Compares two strings in time that doesn't depend on where they differ.
pub fn constant_time_eq(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.bytes()
            .zip(b.bytes())
            .fold(0u8, |acc, (x, y)| acc | (x ^ y))
            == 0
}

fn clean_name(name: &str) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 100 || name.chars().any(char::is_control) {
        return Err("name must be 1 to 100 characters".into());
    }
    Ok(name.to_string())
}

pub struct TokenStore {
    path: PathBuf,
    records: Mutex<Vec<TokenRecord>>,
}

impl TokenStore {
    /// Loads the tokens. A damaged file is moved aside (not overwritten) and the
    /// store starts empty, so the speaker can be set up again.
    pub fn load(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let records = match std::fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_else(|e| {
                let aside = path.with_extension("json.damaged");
                eprintln!(
                    "api-server: token file is damaged ({e}); moved to {}",
                    aside.display()
                );
                let _ = std::fs::rename(&path, aside);
                Vec::new()
            }),
            Err(_) => Vec::new(),
        };
        TokenStore {
            path,
            records: Mutex::new(records),
        }
    }

    fn save(&self, records: &[TokenRecord]) -> Result<(), String> {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let dir = self.path.parent().map(PathBuf::from).unwrap_or_default();
        std::fs::create_dir_all(&dir).map_err(|e| format!("cannot save tokens: {e}"))?;
        let temp = self.path.with_extension("json.tmp");
        let json = serde_json::to_vec_pretty(records).map_err(|e| e.to_string())?;
        std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&temp)
            .and_then(|mut f| f.write_all(&json))
            .and_then(|()| std::fs::rename(&temp, &self.path))
            .map_err(|e| format!("cannot save tokens: {e}"))
    }

    /// Creates a token and returns it with its secret, which is never shown again.
    pub fn create(&self, name: &str, admin: bool) -> Result<(TokenInfo, String), String> {
        let name = clean_name(name)?;
        let secret = format!("kds_{}", random_hex(32));
        let record = TokenRecord {
            id: random_hex(6),
            name,
            hash: hash(&secret),
            admin,
            created: now(),
            last_used: None,
        };
        let mut records = self.records.lock().map_err(|_| "token store unavailable")?;
        if records.len() >= 100 {
            return Err("too many tokens (100)".into());
        }
        records.push(record.clone());
        if let Err(e) = self.save(&records) {
            records.pop();
            return Err(e);
        }
        Ok((TokenInfo::from(&record), secret))
    }

    /// The token with this secret, if any. Records when it was used.
    pub fn verify(&self, secret: &str) -> Option<TokenInfo> {
        let wanted = hash(secret);
        let mut records = self.records.lock().ok()?;
        let index = records
            .iter()
            .position(|r| constant_time_eq(&r.hash, &wanted))?;
        let t = now();
        let stale = records[index]
            .last_used
            .is_none_or(|u| t.saturating_sub(u) >= LAST_USED_RESOLUTION);
        if stale {
            records[index].last_used = Some(t);
            let _ = self.save(&records);
        }
        Some(TokenInfo::from(&records[index]))
    }

    pub fn list(&self) -> Vec<TokenInfo> {
        self.records
            .lock()
            .map(|r| r.iter().map(TokenInfo::from).collect())
            .unwrap_or_default()
    }

    pub fn has_admin(&self) -> bool {
        self.records
            .lock()
            .map(|r| r.iter().any(|t| t.admin))
            .unwrap_or(false)
    }

    /// Revokes a token. The last admin token can't be revoked (AUTH_REQ_002).
    pub fn revoke(&self, id: &str) -> Result<(), String> {
        let mut records = self.records.lock().map_err(|_| "token store unavailable")?;
        let index = records
            .iter()
            .position(|r| r.id == id)
            .ok_or_else(|| format!("no token {id}"))?;
        if records[index].admin && records.iter().filter(|r| r.admin).count() == 1 {
            return Err("the last admin token cannot be revoked".into());
        }
        let removed = records.remove(index);
        if let Err(e) = self.save(&records) {
            records.insert(index, removed);
            return Err(e);
        }
        Ok(())
    }
}

/// Paths that need no token (AUTH_REQ_001).
fn is_open(path: &str) -> bool {
    matches!(path, "/health" | "/api/setup" | "/api/setup/start")
}

fn unauthorized() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        [(header::WWW_AUTHENTICATE, "Bearer")],
        Json(json!({ "error": "missing or invalid token" })),
    )
        .into_response()
}

/// Identifies who made the request and refuses it without a valid token.
pub async fn authenticate(
    State(state): State<AppState>,
    peer: Option<ConnectInfo<SocketAddr>>,
    transport: Option<Extension<Transport>>,
    mut request: Request,
    next: Next,
) -> Response {
    let local = matches!(transport, Some(Extension(Transport::Http)))
        && peer.is_some_and(|ConnectInfo(addr)| addr.ip().is_loopback());
    let principal = if local {
        Some(Principal::Local)
    } else {
        request
            .headers()
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .and_then(|secret| state.tokens.verify(secret.trim()))
            .map(Principal::Token)
    };
    match principal {
        Some(p) => {
            request.extensions_mut().insert(p);
            next.run(request).await
        }
        None if is_open(request.uri().path()) => next.run(request).await,
        None => unauthorized(),
    }
}

/// Checks for an admin token over HTTPS, or the device itself (AUTH_REQ_002):
/// for handlers whose other methods need less, such as the volume limit.
pub fn ensure_admin(
    principal: Option<&Principal>,
    transport: Option<&Transport>,
) -> Result<(), (StatusCode, Json<serde_json::Value>)> {
    match principal {
        Some(Principal::Local) => Ok(()),
        Some(p) if !p.is_admin() => Err((
            StatusCode::FORBIDDEN,
            Json(json!({"error": "an admin token is required"})),
        )),
        Some(_) if transport != Some(&Transport::Https) => Err((
            StatusCode::FORBIDDEN,
            Json(json!({"error": "this endpoint requires HTTPS"})),
        )),
        Some(_) => Ok(()),
        None => Err((
            StatusCode::UNAUTHORIZED,
            Json(json!({"error": "missing or invalid token"})),
        )),
    }
}

/// For token management and credentials: an admin token over HTTPS, or the
/// device itself (AUTH_REQ_002).
pub async fn require_admin(
    principal: Option<Extension<Principal>>,
    transport: Option<Extension<Transport>>,
    request: Request,
    next: Next,
) -> Response {
    let refuse = |status: StatusCode, message: &str| {
        (status, Json(json!({ "error": message }))).into_response()
    };
    match principal {
        Some(Extension(Principal::Local)) => next.run(request).await,
        Some(Extension(p)) if !p.is_admin() => {
            refuse(StatusCode::FORBIDDEN, "an admin token is required")
        }
        Some(Extension(_)) if !matches!(transport, Some(Extension(Transport::Https))) => {
            refuse(StatusCode::FORBIDDEN, "this endpoint requires HTTPS")
        }
        Some(Extension(_)) => next.run(request).await,
        None => unauthorized(),
    }
}
