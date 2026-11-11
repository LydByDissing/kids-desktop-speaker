//! kds-wifi-setup: opens the setup access point and its captive portal when
//! the speaker has no working Wi-Fi, and returns to the home network
//! (NET_ONB_001/002, DEC-012, DEC-044).

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use wifi_setup::monitor::{Decision, Monitor, CHECK_EVERY};
use wifi_setup::portal::{router, Portal};

const BOOT_CONF: &str = "/boot/firmware/kds/wifi.conf";
const LIVE_CONF: &str = "/etc/wpa_supplicant/wpa_supplicant-wlan0.conf";
const AP_UNIT: &str = "kds-setup-ap";
const AP_SCRIPT: &str = "/usr/local/bin/kds-setup-ap.sh";
const PORTAL_ADDR: &str = "192.168.4.1:80";
/// Writing a number of seconds here opens the access point that long (tests).
const REQUEST_FILE: &str = "/run/kds/wifi-setup-request";

fn saved_network(path: &Path) -> bool {
    std::fs::read_to_string(path).is_ok_and(|c| c.contains("network={"))
}

fn output(cmd: &str, args: &[&str]) -> String {
    Command::new(cmd)
        .args(args)
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default()
}

/// Associated and with an IPv4 address.
fn connected() -> bool {
    output("iw", &["dev", "wlan0", "link"]).contains("Connected to")
        && output("ip", &["-4", "addr", "show", "wlan0"]).contains("inet ")
}

/// The networks in range, strongest first, from wpa_supplicant before the
/// access point opens (`iw scan` never finishes while it runs).
fn scan() -> Vec<String> {
    output("wpa_cli", &["-i", "wlan0", "scan"]);
    std::thread::sleep(Duration::from_secs(4));
    wifi_setup::wpa::scan_results(&output("wpa_cli", &["-i", "wlan0", "scan_results"]))
}

fn hostname() -> String {
    std::fs::read_to_string("/proc/sys/kernel/hostname")
        .unwrap_or_default()
        .trim()
        .to_string()
}

/// The access point's name: KidsSpeaker-xxxx from kidsspeaker-xxxx.
fn ap_name(host: &str) -> String {
    match host.strip_prefix("kidsspeaker") {
        Some(rest) => format!("KidsSpeaker{rest}"),
        None => "KidsSpeaker".to_string(),
    }
}

async fn announce(text: &str) {
    let request = serde_json::json!({"cmd": "announce", "text": text, "lang": "da"});
    let _ = kds_ipc::request("/run/kds/audio.sock", &request).await;
}

fn ap_running() -> bool {
    Command::new("systemctl")
        .args(["is-active", "--quiet", AP_UNIT])
        .status()
        .is_ok_and(|s| s.success())
}

/// A transient unit stopped by its time limit stays loaded as "failed", and
/// systemd-run won't reuse the name until that is cleared.
fn reset_ap_unit() {
    let _ = Command::new("systemctl")
        .args(["reset-failed", AP_UNIT])
        .status();
}

fn start_ap(ssid: &str, limit: Option<Duration>) -> bool {
    reset_ap_unit();
    let max = limit
        .map(|d| format!("RuntimeMaxSec={}", d.as_secs()))
        .unwrap_or_else(|| "RuntimeMaxSec=infinity".into());
    let stop = format!("ExecStopPost={AP_SCRIPT} stop");
    Command::new("systemd-run")
        .args([
            "--unit",
            AP_UNIT,
            "--property",
            &max,
            "--property",
            &stop,
            AP_SCRIPT,
            "run",
            ssid,
        ])
        .status()
        .is_ok_and(|s| s.success())
}

fn stop_ap() {
    let _ = Command::new("systemctl").args(["stop", AP_UNIT]).status();
    reset_ap_unit();
}

/// Opens the access point and serves the portal until a network is saved or
/// the access point closes by itself.
async fn setup_mode(limit: Option<Duration>) {
    let host = hostname();
    let ssid = ap_name(&host);
    let networks = Arc::new(Mutex::new(scan()));
    println!("kds-wifi-setup: opening the setup access point {ssid} ({limit:?})");
    announce(&format!(
        "Forbind til netværket {ssid} for at sætte højttaleren op."
    ))
    .await;
    if !start_ap(&ssid, limit) {
        eprintln!("kds-wifi-setup: could not start the access point");
        return;
    }
    let (saved_tx, mut saved_rx) = mpsc::channel::<String>(1);
    let portal = Portal {
        boot_conf: PathBuf::from(BOOT_CONF),
        live_conf: PathBuf::from(LIVE_CONF),
        networks,
        saved: saved_tx,
        name: ssid.clone(),
    };
    // The portal's address exists only while the access point is up.
    let mut server = None;
    for _ in 0..30 {
        if let Ok(listener) = tokio::net::TcpListener::bind(PORTAL_ADDR).await {
            let app = router(portal.clone());
            server = Some(tokio::spawn(async move {
                let _ = axum::serve(listener, app).await;
            }));
            break;
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    loop {
        tokio::select! {
            Some(network) = saved_rx.recv() => {
                println!("kds-wifi-setup: network {network:?} saved; leaving setup mode");
                // Let the phone receive the confirmation page first.
                tokio::time::sleep(Duration::from_secs(3)).await;
                stop_ap();
                break;
            }
            _ = tokio::time::sleep(Duration::from_secs(5)) => {
                if !ap_running() {
                    println!("kds-wifi-setup: the access point closed; trying the saved network");
                    reset_ap_unit();
                    break;
                }
            }
        }
    }
    if let Some(server) = server {
        server.abort();
    }
}

#[tokio::main]
async fn main() {
    // A leftover access point (a crash) is closed first.
    if ap_running() {
        stop_ap();
    }
    let mut monitor = Monitor::new(Instant::now());
    loop {
        let request = std::fs::read_to_string(REQUEST_FILE).ok();
        let decision = match request.and_then(|s| s.trim().parse::<u64>().ok()) {
            Some(secs) => {
                let _ = std::fs::remove_file(REQUEST_FILE);
                Decision::OpenAp(Some(Duration::from_secs(secs)))
            }
            None => monitor.check(
                Instant::now(),
                saved_network(Path::new(BOOT_CONF)),
                connected(),
            ),
        };
        if let Decision::OpenAp(limit) = decision {
            setup_mode(limit).await;
            monitor.restart(Instant::now());
        }
        tokio::time::sleep(CHECK_EVERY).await;
    }
}
