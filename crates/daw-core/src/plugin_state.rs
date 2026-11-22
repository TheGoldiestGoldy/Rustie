//! Versioned, size-limited project representation for opaque plugin-native state.
//!
//! Plugin state is an untrusted byte blob produced by third-party code. It is
//! stored as hex so the project remains ordinary JSON, and every read is
//! validated before it can be handed back to a plugin worker. A malformed or
//! oversized blob is rejected without touching clips, routing or other devices.
use serde::{Deserialize, Serialize};

/// Current on-disk representation version. Bump only with a migration.
pub const PLUGIN_STATE_VERSION: u16 = 1;
/// Hard upper bound for a single plugin state blob (raw bytes, before hex).
/// Large sample libraries are never embedded here; states above this limit are
/// reported and skipped instead of inflating or corrupting a project.
pub const MAX_PLUGIN_STATE_BYTES: usize = 262_144;

pub const FORMAT_CLAP: &str = "clap";
pub const FORMAT_VST3: &str = "vst3";

/// Opaque, plugin-owned state serialized into a project file.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginStateBlob {
    /// Representation version, not the plugin's own version.
    #[serde(default = "default_version")]
    pub version: u16,
    /// `clap` or `vst3`; prevents handing a blob to the wrong host format.
    #[serde(default)]
    pub format: String,
    /// Best-effort plugin identity (CLAP class id or VST3 class uid).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub plugin_uid: String,
    /// Rate at which the state was captured. Informational; restore still
    /// re-activates at the project's current rate.
    #[serde(default)]
    pub sample_rate: u32,
    /// Lower-case hex of the raw plugin state bytes.
    #[serde(default)]
    pub data_hex: String,
}

fn default_version() -> u16 {
    PLUGIN_STATE_VERSION
}

impl PluginStateBlob {
    /// Build a validated blob from raw worker output.
    pub fn from_bytes(
        format: impl Into<String>,
        plugin_uid: impl Into<String>,
        sample_rate: u32,
        data: &[u8],
    ) -> Result<Self, String> {
        let format = format.into();
        if format != FORMAT_CLAP && format != FORMAT_VST3 {
            return Err(format!("Unsupported plugin state format: {format}"));
        }
        if data.len() > MAX_PLUGIN_STATE_BYTES {
            return Err(format!(
                "Plugin state is {} bytes; limit is {MAX_PLUGIN_STATE_BYTES}",
                data.len()
            ));
        }
        Ok(Self {
            version: PLUGIN_STATE_VERSION,
            format,
            plugin_uid: plugin_uid.into(),
            sample_rate,
            data_hex: hex_encode(data),
        })
    }

    /// Validate and decode this blob for the given host format.
    pub fn decode(&self, expected_format: &str) -> Result<Vec<u8>, String> {
        self.validate(expected_format)?;
        hex_decode(&self.data_hex)
    }

    fn validate(&self, expected_format: &str) -> Result<(), String> {
        if self.version != PLUGIN_STATE_VERSION {
            return Err(format!(
                "Unsupported plugin state version {} (expected {PLUGIN_STATE_VERSION})",
                self.version
            ));
        }
        if self.format != expected_format {
            return Err(format!(
                "Plugin state format '{}' does not match '{}'",
                self.format, expected_format
            ));
        }
        if self.sample_rate != 0 && !(8_000..=192_000).contains(&self.sample_rate) {
            return Err(format!(
                "Plugin state sample rate {} is outside 8000..=192000",
                self.sample_rate
            ));
        }
        if !self.data_hex.len().is_multiple_of(2) {
            return Err("Plugin state hex has an odd length".into());
        }
        if self.data_hex.len() / 2 > MAX_PLUGIN_STATE_BYTES {
            return Err(format!(
                "Plugin state exceeds the {MAX_PLUGIN_STATE_BYTES}-byte limit"
            ));
        }
        // Reject non-hex characters before any plugin ever sees the bytes.
        if !self.data_hex.as_bytes().iter().all(u8::is_ascii_hexdigit) {
            return Err("Plugin state contains non-hex characters".into());
        }
        Ok(())
    }

    pub fn is_empty(&self) -> bool {
        self.data_hex.is_empty()
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &byte in bytes {
        out.push(DIGITS[(byte >> 4) as usize] as char);
        out.push(DIGITS[(byte & 0x0f) as usize] as char);
    }
    out
}

fn hex_decode(text: &str) -> Result<Vec<u8>, String> {
    if !text.len().is_multiple_of(2) {
        return Err("Plugin state hex has an odd length".into());
    }
    let mut out = Vec::with_capacity(text.len() / 2);
    let bytes = text.as_bytes();
    for pair in bytes.as_chunks::<2>().0 {
        let hi = hex_value(pair[0]).ok_or("Invalid plugin state hex digit")?;
        let lo = hex_value(pair[1]).ok_or("Invalid plugin state hex digit")?;
        out.push((hi << 4) | lo);
    }
    Ok(out)
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_round_trips_with_version_format_and_identity() {
        let raw = b"\x00\x01native-preset\xff";
        let blob = PluginStateBlob::from_bytes(FORMAT_VST3, "ABC123", 44_100, raw).unwrap();
        assert_eq!(blob.version, PLUGIN_STATE_VERSION);
        assert_eq!(blob.decode(FORMAT_VST3).unwrap(), raw);
        assert!(blob.decode(FORMAT_CLAP).is_err());
    }

    #[test]
    fn malformed_and_oversized_state_is_rejected_without_panicking() {
        let mut blob = PluginStateBlob::from_bytes(FORMAT_CLAP, "id", 48_000, b"ok").unwrap();
        blob.version = 99;
        assert!(blob.decode(FORMAT_CLAP).is_err());

        let mut blob = PluginStateBlob::from_bytes(FORMAT_CLAP, "id", 48_000, b"ok").unwrap();
        blob.data_hex = "zz".into();
        assert!(blob.decode(FORMAT_CLAP).is_err());

        let oversized = vec![0_u8; MAX_PLUGIN_STATE_BYTES + 1];
        assert!(PluginStateBlob::from_bytes(FORMAT_CLAP, "id", 48_000, &oversized).is_err());
    }

    #[test]
    fn missing_fields_in_old_state_json_default_safely() {
        let blob: PluginStateBlob = serde_json::from_str("{\"data_hex\":\"00\"}").unwrap();
        assert_eq!(blob.version, PLUGIN_STATE_VERSION);
        assert!(blob.format.is_empty());
        assert!(blob.decode(FORMAT_CLAP).is_err());
    }
}
