//! The BlueZ pairing agent (BT_REQ_002, DEC-035).
//!
//! BlueZ calls these methods when a device pairs or connects. While the pairing
//! window is open the agent accepts the pairing and its profiles (A2DP, AVRCP),
//! and marks the device trusted; iOS drops the audio link if AVRCP is refused.
//! Trusted devices reconnect later without involving the agent. Outside the
//! window, every request reaches the agent only for an unknown device and is
//! rejected.

use crate::pairing::PairingWindow;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use zbus::zvariant::{ObjectPath, OwnedObjectPath, Value};
use zbus::{interface, Connection};

/// Where the agent object lives on the system bus.
pub const AGENT_PATH: &str = "/dk/kidsspeaker/agent";

/// Errors BlueZ understands; `Rejected` refuses the request.
#[derive(Debug, zbus::DBusError)]
#[zbus(prefix = "org.bluez.Error")]
pub enum AgentError {
    #[zbus(error)]
    ZBus(zbus::Error),
    Rejected(String),
}

pub struct Agent {
    pub window: Arc<Mutex<PairingWindow>>,
    /// BlueZ's bus name; "org.bluez" except in tests.
    pub bluez: String,
}

impl Agent {
    fn window_open(&self) -> bool {
        self.window
            .lock()
            .map(|w| w.is_open(Instant::now()))
            .unwrap_or(false)
    }

    /// Marks the device trusted, so it reconnects later without the agent
    /// (BlueZ authorises trusted devices itself). This runs as its own task:
    /// the agent answers BlueZ first and never waits on BlueZ while BlueZ waits
    /// on it.
    fn trust_soon(&self, bus: &Connection, device: &ObjectPath<'_>) {
        let bus = bus.clone();
        let bluez = self.bluez.clone();
        let device = device.to_owned();
        tokio::spawn(async move {
            let result = bus
                .call_method(
                    Some(bluez.as_str()),
                    &device,
                    Some("org.freedesktop.DBus.Properties"),
                    "Set",
                    &("org.bluez.Device1", "Trusted", Value::from(true)),
                )
                .await;
            match result {
                Ok(_) => println!("bluetooth-service: trusted {device}"),
                Err(e) => eprintln!("bluetooth-service: could not trust {device}: {e}"),
            }
        });
    }

    /// Accepts during the pairing window and trusts the device; rejects otherwise.
    fn accept_new(
        &self,
        bus: &Connection,
        device: &ObjectPath<'_>,
        what: &str,
    ) -> Result<(), AgentError> {
        if !self.window_open() {
            eprintln!("bluetooth-service: rejected {what} from {device}: pairing window closed");
            return Err(AgentError::Rejected("pairing window closed".into()));
        }
        println!("bluetooth-service: accepted {what} from {device}");
        self.trust_soon(bus, device);
        Ok(())
    }
}

#[interface(name = "org.bluez.Agent1")]
impl Agent {
    async fn release(&self) {
        println!("bluetooth-service: agent released by BlueZ");
    }

    async fn request_pin_code(
        &self,
        #[zbus(connection)] bus: &Connection,
        device: OwnedObjectPath,
    ) -> Result<String, AgentError> {
        self.accept_new(bus, &device, "PIN request")?;
        Ok("0000".into())
    }

    async fn display_pin_code(&self, _device: OwnedObjectPath, _pincode: String) {}

    async fn request_passkey(
        &self,
        #[zbus(connection)] bus: &Connection,
        device: OwnedObjectPath,
    ) -> Result<u32, AgentError> {
        self.accept_new(bus, &device, "passkey request")?;
        Ok(0)
    }

    async fn display_passkey(&self, _device: OwnedObjectPath, _passkey: u32, _entered: u16) {}

    async fn request_confirmation(
        &self,
        #[zbus(connection)] bus: &Connection,
        device: OwnedObjectPath,
        _passkey: u32,
    ) -> Result<(), AgentError> {
        self.accept_new(bus, &device, "pairing")
    }

    async fn request_authorization(
        &self,
        #[zbus(connection)] bus: &Connection,
        device: OwnedObjectPath,
    ) -> Result<(), AgentError> {
        self.accept_new(bus, &device, "pairing")
    }

    /// BlueZ asks only about devices that aren't trusted yet; it authorises
    /// trusted ones itself. So a service is authorised during the pairing window
    /// (the device has just paired) and rejected otherwise.
    async fn authorize_service(
        &self,
        #[zbus(connection)] bus: &Connection,
        device: OwnedObjectPath,
        uuid: String,
    ) -> Result<(), AgentError> {
        self.accept_new(bus, &device, &format!("service {uuid}"))
    }

    async fn cancel(&self) {}
}
