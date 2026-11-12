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

/// What a phone's audio transport reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransportEvent {
    /// Started (State "active") or stopped ("idle") streaming.
    Streaming(bool),
    /// Its AVRCP absolute volume, 0-127: the phone's volume buttons (DEC-049).
    Volume(u16),
}

/// The events in one `MediaTransport1` PropertiesChanged.
pub fn transport_events(
    changed: &std::collections::HashMap<String, zbus::zvariant::OwnedValue>,
) -> Vec<TransportEvent> {
    let mut events = Vec::new();
    if let Some(state) = changed
        .get("State")
        .and_then(|v| String::try_from(v.clone()).ok())
    {
        match state.as_str() {
            "active" => events.push(TransportEvent::Streaming(true)),
            "idle" => events.push(TransportEvent::Streaming(false)),
            _ => {}
        }
    }
    if let Some(volume) = changed.get("Volume").and_then(|v| u16::try_from(v).ok()) {
        events.push(TransportEvent::Volume(volume.min(127)));
    }
    events
}

/// Watches BlueZ's audio transports and reports when a phone starts or stops
/// streaming and when it changes its volume. Returns when the bus closes.
pub async fn watch_transports<F, Fut>(bus: &Connection, bluez: &str, report: F) -> zbus::Result<()>
where
    F: Fn(TransportEvent) -> Fut,
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
        for event in transport_events(&changed) {
            report(event).await;
        }
    }
    Ok(())
}

#[cfg(test)]
mod transport_tests {
    use super::*;
    use std::collections::HashMap;
    use zbus::zvariant::{OwnedValue, Value};

    fn changed(pairs: &[(&str, Value<'_>)]) -> HashMap<String, OwnedValue> {
        pairs
            .iter()
            .map(|(k, v)| {
                (
                    k.to_string(),
                    OwnedValue::try_from(v.try_clone().unwrap()).unwrap(),
                )
            })
            .collect()
    }

    #[test]
    fn streaming_and_volume_are_reported() {
        assert_eq!(
            transport_events(&changed(&[("State", Value::from("active"))])),
            vec![TransportEvent::Streaming(true)]
        );
        assert_eq!(
            transport_events(&changed(&[("State", Value::from("idle"))])),
            vec![TransportEvent::Streaming(false)]
        );
        // The phone's volume buttons: BlueZ's Volume is a uint16 (0-127).
        assert_eq!(
            transport_events(&changed(&[("Volume", Value::from(64u16))])),
            vec![TransportEvent::Volume(64)]
        );
        assert_eq!(
            transport_events(&changed(&[
                ("State", Value::from("active")),
                ("Volume", Value::from(200u16))
            ])),
            vec![TransportEvent::Streaming(true), TransportEvent::Volume(127)]
        );
        assert!(transport_events(&changed(&[("State", Value::from("pending"))])).is_empty());
        assert!(transport_events(&changed(&[("Delay", Value::from(5u16))])).is_empty());
    }
}
