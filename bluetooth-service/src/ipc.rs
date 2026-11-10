//! The Bluetooth Service's IPC server: `status` and `pair` (IPC protocol,
//! DEC-027, BT_REQ_002).

use crate::bluez;
use crate::pairing::{PairingWindow, DEFAULT_WINDOW};
use kds_ipc::{command, error, ok};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};
use zbus::Connection;

/// What the IPC handlers share with the rest of the service.
#[derive(Clone)]
pub struct Context {
    pub connected: Arc<RwLock<bool>>,
    pub window: Arc<Mutex<PairingWindow>>,
    /// The system bus and BlueZ's name, when BlueZ is reachable.
    pub bus: Option<Connection>,
    pub bluez: String,
    pub adapter: String,
}

impl Context {
    pub fn new(connected: Arc<RwLock<bool>>, window: Arc<Mutex<PairingWindow>>) -> Self {
        Context {
            connected,
            window,
            bus: None,
            bluez: "org.bluez".into(),
            adapter: bluez::ADAPTER_PATH.into(),
        }
    }
}

pub async fn serve(sock_path: &str, ctx: Context) -> std::io::Result<()> {
    kds_ipc::serve(sock_path, move |request| {
        let ctx = ctx.clone();
        async move { handle(request, &ctx).await }
    })
    .await
}

fn pairing_status(window: &Mutex<PairingWindow>) -> Value {
    let now = Instant::now();
    match window.lock() {
        Ok(w) => json!({ "open": w.is_open(now), "remaining_s": w.remaining(now).as_secs() }),
        Err(_) => json!({ "open": false, "remaining_s": 0 }),
    }
}

pub async fn handle(request: Value, ctx: &Context) -> Value {
    match command(&request) {
        Some("status") => match ctx.connected.read() {
            Ok(is_connected) => ok(json!({
                "connected": *is_connected,
                "controller": null,
                "pairing": pairing_status(&ctx.window),
            })),
            Err(_) => error("bluetooth state unavailable"),
        },
        Some("pair") => {
            let requested = match request.get("seconds") {
                None | Some(Value::Null) => DEFAULT_WINDOW,
                Some(v) => match v.as_u64() {
                    Some(s) if s > 0 => Duration::from_secs(s),
                    _ => return error("seconds must be a positive integer"),
                },
            };
            let Some(bus) = &ctx.bus else {
                return error("bluetooth unavailable");
            };
            let window = match ctx.window.lock() {
                Ok(mut w) => w.open(Instant::now(), requested),
                Err(_) => return error("bluetooth state unavailable"),
            };
            if let Err(e) = bluez::open_pairing(bus, &ctx.bluez, &ctx.adapter, window).await {
                if let Ok(mut w) = ctx.window.lock() {
                    w.close();
                }
                return error(format!("could not open pairing: {e}"));
            }
            println!(
                "bluetooth-service: pairing window open for {}s",
                window.as_secs()
            );
            ok(json!({ "pairing": pairing_status(&ctx.window) }))
        }
        Some(other) => error(format!("unknown command: {other}")),
        None => error("missing cmd"),
    }
}
