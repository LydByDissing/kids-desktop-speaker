//! Talking to BlueZ over D-Bus: register the agent and open the pairing window.

use std::time::Duration;
use zbus::zvariant::{ObjectPath, Value};
use zbus::Connection;

/// The Pi Zero W's only adapter.
pub const ADAPTER_PATH: &str = "/org/bluez/hci0";

/// Registers the agent at `agent_path` with BlueZ and makes it the default, so
/// it answers every pairing and authorisation request.
pub async fn register_agent(bus: &Connection, bluez: &str, agent_path: &str) -> zbus::Result<()> {
    let path = ObjectPath::try_from(agent_path)?;
    bus.call_method(
        Some(bluez),
        "/org/bluez",
        Some("org.bluez.AgentManager1"),
        "RegisterAgent",
        &(&path, "NoInputNoOutput"),
    )
    .await?;
    bus.call_method(
        Some(bluez),
        "/org/bluez",
        Some("org.bluez.AgentManager1"),
        "RequestDefaultAgent",
        &(&path,),
    )
    .await?;
    Ok(())
}

async fn set_adapter(
    bus: &Connection,
    bluez: &str,
    adapter: &str,
    name: &str,
    value: Value<'_>,
) -> zbus::Result<()> {
    bus.call_method(
        Some(bluez),
        adapter,
        Some("org.freedesktop.DBus.Properties"),
        "Set",
        &("org.bluez.Adapter1", name, value),
    )
    .await?;
    Ok(())
}

/// Makes the adapter discoverable and pairable for `window`. BlueZ turns both
/// off again when the timeouts expire.
pub async fn open_pairing(
    bus: &Connection,
    bluez: &str,
    adapter: &str,
    window: Duration,
) -> zbus::Result<()> {
    let secs = window.as_secs() as u32;
    set_adapter(bus, bluez, adapter, "PairableTimeout", Value::from(secs)).await?;
    set_adapter(
        bus,
        bluez,
        adapter,
        "DiscoverableTimeout",
        Value::from(secs),
    )
    .await?;
    set_adapter(bus, bluez, adapter, "Pairable", Value::from(true)).await?;
    set_adapter(bus, bluez, adapter, "Discoverable", Value::from(true)).await?;
    Ok(())
}

type ManagedObjects = std::collections::HashMap<
    zbus::zvariant::OwnedObjectPath,
    std::collections::HashMap<
        String,
        std::collections::HashMap<String, zbus::zvariant::OwnedValue>,
    >,
>;

/// The phones' AVRCP players (``org.bluez.MediaPlayer1``) with their status.
pub async fn players(bus: &Connection, bluez: &str) -> zbus::Result<Vec<(String, String)>> {
    let reply = bus
        .call_method(
            Some(bluez),
            "/",
            Some("org.freedesktop.DBus.ObjectManager"),
            "GetManagedObjects",
            &(),
        )
        .await?;
    let objects: ManagedObjects = reply.body().deserialize()?;
    let mut found: Vec<(String, String)> = objects
        .into_iter()
        .filter_map(|(path, interfaces)| {
            let player = interfaces.get("org.bluez.MediaPlayer1")?;
            let status = player
                .get("Status")
                .and_then(|v| String::try_from(v.clone()).ok())
                .unwrap_or_default();
            Some((path.to_string(), status))
        })
        .collect();
    found.sort();
    Ok(found)
}

/// Sends AVRCP Pause or Play to the connected phone (DEC-039). Pause is only
/// sent to a player that is playing, so a later Play resumes only what this
/// paused; it is an error when nothing plays.
pub async fn avrcp(bus: &Connection, bluez: &str, action: &str) -> Result<(), String> {
    let method = match action {
        "pause" => "Pause",
        "play" => "Play",
        other => return Err(format!("unknown action: {other}")),
    };
    let players = players(bus, bluez).await.map_err(|e| e.to_string())?;
    let target = if action == "pause" {
        players.iter().find(|(_, status)| status == "playing")
    } else {
        players.first()
    }
    .ok_or_else(|| {
        if action == "pause" {
            "nothing is playing".to_string()
        } else {
            "no phone connected".to_string()
        }
    })?;
    let path = ObjectPath::try_from(target.0.as_str()).map_err(|e| e.to_string())?;
    bus.call_method(
        Some(bluez),
        &path,
        Some("org.bluez.MediaPlayer1"),
        method,
        &(),
    )
    .await
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// Watches BlueZ's audio transports and calls `report(true)` when a phone
/// starts streaming (State "active") and `report(false)` when it stops
/// ("idle"). Returns when the bus closes.
pub async fn watch_transports<F, Fut>(bus: &Connection, bluez: &str, report: F) -> zbus::Result<()>
where
    F: Fn(bool) -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    use futures_util::StreamExt;
    let rule = zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .sender(bluez)?
        .interface("org.freedesktop.DBus.Properties")?
        .member("PropertiesChanged")?
        .build();
    let mut stream = zbus::MessageStream::for_match_rule(rule, bus, None).await?;
    while let Some(message) = stream.next().await {
        let Ok(message) = message else { continue };
        let body = message.body();
        let Ok((interface, changed, _)) = body.deserialize::<(
            String,
            std::collections::HashMap<String, zbus::zvariant::OwnedValue>,
            Vec<String>,
        )>() else {
            continue;
        };
        if interface != "org.bluez.MediaTransport1" {
            continue;
        }
        if let Some(state) = changed
            .get("State")
            .and_then(|v| String::try_from(v.clone()).ok())
        {
            match state.as_str() {
                "active" => report(true).await,
                "idle" => report(false).await,
                _ => {}
            }
        }
    }
    Ok(())
}
