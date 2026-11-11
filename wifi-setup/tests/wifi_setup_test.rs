//! Wi-Fi onboarding (NET_ONB_001/002, DEC-044).

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tower::ServiceExt;
use wifi_setup::monitor::{Decision, Monitor, AP_TIME, CHECK_EVERY, GRACE, MAX_FAILURES};
use wifi_setup::portal::{escape, router, Portal};
use wifi_setup::wpa::{config, psk, validate};

#[test]
fn the_wpa_key_matches_the_ieee_test_vector() {
    // IEEE 802.11i-2004, H.4.3: passphrase "password", SSID "IEEE".
    assert_eq!(
        psk("IEEE", "password"),
        "f42c6fc52df0ebef9ebb4b90b38a5f902e83fe1b135a70e23aed762e9710a12e"
    );
}

#[test]
fn the_config_stores_the_key_not_the_password() {
    let c = config("Mit \"net\"", "hemmelig123").unwrap();
    assert!(
        c.contains("ssid=4d697420226e657422\n"),
        "the name is hex-encoded: {c}"
    );
    assert!(c.contains(&format!("psk={}\n", psk("Mit \"net\"", "hemmelig123"))));
    assert!(!c.contains("hemmelig123"), "no plaintext password");
    assert!(c.starts_with("ctrl_interface=DIR=/run/wpa_supplicant\ncountry=DK\n"));
    let open = config("Café", "").unwrap();
    assert!(open.contains("key_mgmt=NONE") && !open.contains("psk="));
}

#[test]
fn bad_networks_are_refused() {
    assert!(validate("", "password1").is_err());
    assert!(validate(&"x".repeat(33), "password1").is_err());
    assert!(validate("home", "short").is_err());
    assert!(validate("home", &"x".repeat(64)).is_err());
    assert!(validate("home", "pässword1").is_err(), "non-ASCII");
    assert!(validate("home", "").is_ok(), "an open network");
}

#[test]
fn without_a_saved_network_the_access_point_opens_until_set_up() {
    let t0 = Instant::now();
    let mut m = Monitor::new(t0);
    assert_eq!(m.check(t0, false, false), Decision::OpenAp(None));
}

#[test]
fn five_failed_checks_after_the_grace_open_it_for_ten_minutes() {
    let t0 = Instant::now();
    let mut m = Monitor::new(t0);
    assert_eq!(
        m.check(t0 + Duration::from_secs(60), true, false),
        Decision::Wait,
        "grace at boot"
    );
    let mut t = t0 + GRACE;
    for _ in 1..MAX_FAILURES {
        assert_eq!(m.check(t, true, false), Decision::Wait);
        t += CHECK_EVERY;
    }
    assert_eq!(m.check(t, true, false), Decision::OpenAp(Some(AP_TIME)));
    assert_eq!(AP_TIME, Duration::from_secs(600));
}

#[test]
fn a_connection_resets_the_count() {
    let t0 = Instant::now();
    let mut m = Monitor::new(t0);
    let t = t0 + GRACE;
    for _ in 1..MAX_FAILURES {
        m.check(t, true, false);
    }
    assert_eq!(m.check(t, true, true), Decision::Wait);
    for _ in 1..MAX_FAILURES {
        assert_eq!(
            m.check(t, true, false),
            Decision::Wait,
            "counting starts over"
        );
    }
    m.restart(t);
    assert_eq!(
        m.check(t + Duration::from_secs(10), true, false),
        Decision::Wait,
        "a fresh grace after the AP"
    );
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("kds-wifi-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn portal(dir: &std::path::Path) -> (Portal, mpsc::Receiver<String>) {
    let (tx, rx) = mpsc::channel(1);
    (
        Portal {
            boot_conf: dir.join("boot/kds/wifi.conf"),
            live_conf: dir.join("etc/wpa_supplicant-wlan0.conf"),
            networks: Arc::new(Mutex::new(vec!["mesh".into(), "<script>".into()])),
            saved: tx,
            name: "KidsSpeaker-2df5".into(),
        },
        rx,
    )
}

async fn call(portal: Portal, method: &str, uri: &str, form: Option<&str>) -> (StatusCode, String) {
    let mut request = Request::builder().method(method).uri(uri);
    if form.is_some() {
        request = request.header("content-type", "application/x-www-form-urlencoded");
    }
    let body = form
        .map(|f| Body::from(f.to_string()))
        .unwrap_or_else(Body::empty);
    let response = router(portal)
        .oneshot(request.body(body).unwrap())
        .await
        .unwrap();
    let status = response.status();
    (
        status,
        String::from_utf8(
            response
                .into_body()
                .collect()
                .await
                .unwrap()
                .to_bytes()
                .to_vec(),
        )
        .unwrap(),
    )
}

#[tokio::test]
async fn every_path_shows_the_setup_page() {
    let dir = temp_dir("page");
    let (p, _rx) = portal(&dir);
    for path in ["/", "/generate_204", "/hotspot-detect.html", "/anything"] {
        let (status, body) = call(p.clone(), "GET", path, None).await;
        assert_eq!(status, StatusCode::OK, "{path}");
        assert!(body.contains("Wi-Fi til KidsSpeaker-2df5"), "{path}");
    }
    let (_, body) = call(p, "GET", "/", None).await;
    assert!(body.contains("<option value=\"mesh\">mesh</option>"));
    assert!(
        body.contains("&lt;script&gt;") && !body.contains("<option value=\"<script>\">"),
        "names are escaped"
    );
    assert_eq!(escape("a&b\"'"), "a&amp;b&quot;&#39;");
}

#[tokio::test]
async fn saving_writes_the_boot_partition_and_wpa_supplicant() {
    let dir = temp_dir("save");
    let (p, mut rx) = portal(&dir);
    let (status, body) = call(
        p.clone(),
        "POST",
        "/setup",
        Some("ssid=mesh&other=&password=hemmelig123"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("forbinder nu til <b>mesh</b>"), "{body}");
    assert_eq!(rx.recv().await.as_deref(), Some("mesh"));
    for path in [&p.boot_conf, &p.live_conf] {
        let saved = std::fs::read_to_string(path).unwrap();
        assert_eq!(saved, config("mesh", "hemmelig123").unwrap());
        assert_eq!(
            std::fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}

#[tokio::test]
async fn another_network_can_be_typed_and_bad_input_saves_nothing() {
    let dir = temp_dir("other");
    let (p, mut rx) = portal(&dir);
    let (_, body) = call(
        p.clone(),
        "POST",
        "/setup",
        Some("ssid=mesh&other=Hytten&password=short"),
    )
    .await;
    assert!(body.contains("8 to 63"), "the error is shown: {body}");
    assert!(!p.boot_conf.exists(), "nothing saved");
    call(
        p.clone(),
        "POST",
        "/setup",
        Some("ssid=mesh&other=Hytten&password="),
    )
    .await;
    assert_eq!(
        rx.recv().await.as_deref(),
        Some("Hytten"),
        "the typed name wins"
    );
    assert!(std::fs::read_to_string(&p.boot_conf)
        .unwrap()
        .contains("key_mgmt=NONE"));
}

#[test]
fn scan_results_are_listed_strongest_first() {
    let text = "bssid / frequency / signal level / flags / ssid\n\
                aa:bb:cc:00:00:01\t2437\t-71\t[WPA2-PSK-CCMP][ESS]\tNabo\n\
                aa:bb:cc:00:00:02\t2457\t-42\t[WPA2-PSK-CCMP][ESS]\tmesh\n\
                aa:bb:cc:00:00:03\t5180\t-60\t[WPA2-PSK-CCMP][ESS]\tmesh\n\
                aa:bb:cc:00:00:04\t2412\t-50\t[ESS]\t\n";
    assert_eq!(
        wifi_setup::wpa::scan_results(text),
        vec!["mesh", "Nabo"],
        "deduplicated, hidden skipped"
    );
    assert!(wifi_setup::wpa::scan_results("").is_empty());
}
