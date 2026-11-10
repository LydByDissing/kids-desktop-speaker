//! The BlueZ pairing agent (BT_REQ_002, DEC-035).
//!
//! BlueZ calls these methods when a device pairs or connects. While the pairing
//! window is open the agent accepts the pairing and marks the device trusted, so
//! its profiles (A2DP, AVRCP) are authorised at once; iOS drops the audio link if
//! AVRCP is refused. Outside the window, only trusted devices are authorised.

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
    /// The bus to reach BlueZ's device objects on.
    pub bus: Connection,
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

    async fn device_property(&self, device: &ObjectPath<'_>, name: &str) -> zbus::Result<bool> {
        let reply = self
            .bus
            .call_method(
                Some(self.bluez.as_str()),
                device,
                Some("org.freedesktop.DBus.Properties"),
                "Get",
                &("org.bluez.Device1", name),
            )
            .await?;
        let value: zbus::zvariant::OwnedValue = reply.body().deserialize()?;
        bool::try_from(value).map_err(zbus::Error::Variant)
    }

    async fn set_trusted(&self, device: &ObjectPath<'_>) -> zbus::Result<()> {
        self.bus
            .call_method(
                Some(self.bluez.as_str()),
                device,
                Some("org.freedesktop.DBus.Properties"),
                "Set",
                &("org.bluez.Device1", "Trusted", Value::from(true)),
            )
            .await?;
        Ok(())
    }

    /// Accepts during the pairing window and trusts the device; rejects otherwise.
    async fn accept_new(&self, device: &ObjectPath<'_>, what: &str) -> Result<(), AgentError> {
        if !self.window_open() {
            eprintln!("bluetooth-service: rejected {what} from {device}: pairing window closed");
            return Err(AgentError::Rejected("pairing window closed".into()));
        }
        self.set_trusted(device).await?;
        println!("bluetooth-service: accepted {what} from {device} and trusted it");
        Ok(())
    }
}

#[interface(name = "org.bluez.Agent1")]
impl Agent {
    async fn release(&self) {
        println!("bluetooth-service: agent released by BlueZ");
    }

    async fn request_pin_code(&self, device: OwnedObjectPath) -> Result<String, AgentError> {
        self.accept_new(&device, "PIN request").await?;
        Ok("0000".into())
    }

    async fn display_pin_code(&self, _device: OwnedObjectPath, _pincode: String) {}

    async fn request_passkey(&self, device: OwnedObjectPath) -> Result<u32, AgentError> {
        self.accept_new(&device, "passkey request").await?;
        Ok(0)
    }

    async fn display_passkey(&self, _device: OwnedObjectPath, _passkey: u32, _entered: u16) {}

    async fn request_confirmation(
        &self,
        device: OwnedObjectPath,
        _passkey: u32,
    ) -> Result<(), AgentError> {
        self.accept_new(&device, "pairing").await
    }

    async fn request_authorization(&self, device: OwnedObjectPath) -> Result<(), AgentError> {
        self.accept_new(&device, "pairing").await
    }

    /// A trusted device's services are always authorised; an unknown device's
    /// only during the pairing window.
    async fn authorize_service(
        &self,
        device: OwnedObjectPath,
        uuid: String,
    ) -> Result<(), AgentError> {
        if self
            .device_property(&device, "Trusted")
            .await
            .unwrap_or(false)
        {
            return Ok(());
        }
        self.accept_new(&device, &format!("service {uuid}")).await
    }

    async fn cancel(&self) {}
}
