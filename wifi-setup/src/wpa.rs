//! The wpa_supplicant configuration for a home network (DEC-044).

/// Checks a network name and password as a phone would let you enter them.
pub fn validate(ssid: &str, password: &str) -> Result<(), String> {
    if ssid.is_empty() || ssid.len() > 32 {
        return Err("the network name must be 1 to 32 bytes".into());
    }
    if !password.is_empty() && !(8..=63).contains(&password.len()) {
        return Err(
            "the password must be 8 to 63 characters (or empty for an open network)".into(),
        );
    }
    if password
        .chars()
        .any(|c| (c as u32) < 0x20 || (c as u32) > 0x7e)
    {
        return Err("the password may only use printable ASCII characters".into());
    }
    Ok(())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// The 256-bit WPA key for a network (PBKDF2-HMAC-SHA1, 4096 rounds, the
/// network name as salt), as `wpa_passphrase` computes it.
pub fn psk(ssid: &str, password: &str) -> String {
    let mut key = [0u8; 32];
    pbkdf2::pbkdf2_hmac::<sha1::Sha1>(password.as_bytes(), ssid.as_bytes(), 4096, &mut key);
    hex(&key)
}

/// The configuration file. The network name is hex-encoded and only the key
/// is stored, so no character in either can break the file.
pub fn config(ssid: &str, password: &str) -> Result<String, String> {
    validate(ssid, password)?;
    let security = if password.is_empty() {
        "\tkey_mgmt=NONE\n".to_string()
    } else {
        format!("\tpsk={}\n", psk(ssid, password))
    };
    Ok(format!(
        "ctrl_interface=DIR=/run/wpa_supplicant\ncountry=DK\nnetwork={{\n\tssid={}\n{security}}}\n",
        hex(ssid.as_bytes())
    ))
}

/// The network names in `wpa_cli scan_results` output, strongest first, without
/// duplicates or hidden networks.
pub fn scan_results(text: &str) -> Vec<String> {
    let mut rows: Vec<(i32, String)> = text
        .lines()
        .skip(1)
        .filter_map(|line| {
            let cols: Vec<&str> = line.split('\t').collect();
            let signal = cols.get(2)?.trim().parse().ok()?;
            let ssid = cols.get(4)?.trim();
            (!ssid.is_empty()).then(|| (signal, ssid.to_string()))
        })
        .collect();
    rows.sort_by_key(|r| std::cmp::Reverse(r.0));
    let mut names: Vec<String> = Vec::new();
    for (_, ssid) in rows {
        if !names.contains(&ssid) {
            names.push(ssid);
        }
    }
    names
}
