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
