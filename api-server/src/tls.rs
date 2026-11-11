//! HTTPS for the API and Web UI (WEB_REQ_001, DEC-032).
//!
//! Each device creates its own self-signed certificate on first boot and keeps
//! it on the data partition, so browsers that accepted it once keep trusting it
//! across updates. It names the device by hostname; if the hostname changes, a
//! new certificate is made.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

pub const TLS_DIR: &str = "/data/tls";

/// The names a device's certificate covers.
pub fn names_for(hostname: &str) -> Vec<String> {
    let host = hostname.trim();
    let mut names = Vec::new();
    if !host.is_empty() {
        names.push(format!("{host}.local"));
        names.push(host.to_string());
    }
    names.push("localhost".to_string());
    names
}

#[derive(Debug, Clone)]
pub struct Certificate {
    pub cert_pem: String,
    pub key_pem: String,
    /// Whether it was created just now (rather than loaded).
    pub created: bool,
}

fn paths(dir: &Path) -> (PathBuf, PathBuf, PathBuf) {
    (dir.join("cert.pem"), dir.join("key.pem"), dir.join("names"))
}

/// Writes `data` to `path` atomically with the given mode.
fn write_private(path: &Path, data: &[u8], mode: u32) -> std::io::Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    let temp = path.with_extension("tmp");
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(mode)
        .open(&temp)?;
    f.write_all(data)?;
    f.sync_all()?;
    fs::rename(&temp, path)
}

/// Loads the device's certificate from `dir`, or creates one for `hostname`
/// when there is none or it was made for other names.
pub fn load_or_create(dir: &Path, hostname: &str) -> anyhow::Result<Certificate> {
    let (cert_path, key_path, names_path) = paths(dir);
    let names = names_for(hostname);
    let wanted = names.join("\n");
    if let (Ok(cert_pem), Ok(key_pem), Ok(saved)) = (
        fs::read_to_string(&cert_path),
        fs::read_to_string(&key_path),
        fs::read_to_string(&names_path),
    ) {
        if saved == wanted && !cert_pem.is_empty() && !key_pem.is_empty() {
            return Ok(Certificate {
                cert_pem,
                key_pem,
                created: false,
            });
        }
    }

    let mut params = rcgen::CertificateParams::new(names.clone())?;
    params.subject_alt_names.push(rcgen::SanType::IpAddress(
        std::net::Ipv4Addr::LOCALHOST.into(),
    ));
    params
        .distinguished_name
        .push(rcgen::DnType::CommonName, names[0].clone());
    params
        .distinguished_name
        .push(rcgen::DnType::OrganizationName, "Kids Desktop Speaker");
    let now = std::time::SystemTime::now();
    params.not_before = (now - std::time::Duration::from_secs(86_400)).into();
    params.not_after = (now + std::time::Duration::from_secs(20 * 365 * 86_400)).into();
    let key = rcgen::KeyPair::generate()?;
    let cert = params.self_signed(&key)?;

    fs::create_dir_all(dir)?;
    fs::set_permissions(dir, std::os::unix::fs::PermissionsExt::from_mode(0o700))?;
    write_private(&key_path, key.serialize_pem().as_bytes(), 0o600)?;
    write_private(&cert_path, cert.pem().as_bytes(), 0o644)?;
    write_private(&names_path, wanted.as_bytes(), 0o644)?;
    Ok(Certificate {
        cert_pem: cert.pem(),
        key_pem: key.serialize_pem(),
        created: true,
    })
}
