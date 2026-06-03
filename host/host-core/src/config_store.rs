use crate::config::HostConfig;
use std::fs;
use std::io;
use std::path::Path;

#[derive(Debug)]
pub enum ConfigStoreError {
    Io(io::Error),
    Json(serde_json::Error),
}

impl From<io::Error> for ConfigStoreError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<serde_json::Error> for ConfigStoreError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

pub fn load_config(
    path: &Path,
    default_password_hash: &str,
) -> Result<HostConfig, ConfigStoreError> {
    if !path.exists() {
        return Ok(HostConfig::new(default_password_hash));
    }

    let json = fs::read_to_string(path)?;
    let mut config: HostConfig = serde_json::from_str(&json)?;
    if config.pairing_password_hash.is_empty() {
        config.pairing_password_hash = default_password_hash.to_string();
    }

    Ok(config)
}

pub fn save_config(path: &Path, config: &HostConfig) -> Result<(), ConfigStoreError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let json = serde_json::to_string_pretty(config)?;
    fs::write(path, json)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pairing::TrustedDevice;
    use std::path::PathBuf;

    #[test]
    fn missing_config_returns_default() {
        let path = temp_config_path("missing");

        let config = load_config(&path, "hash").expect("default config loads");

        assert_eq!(config.pairing_password_hash, "hash");
        assert!(config.trusted_devices.is_empty());
    }

    #[test]
    fn config_round_trips_as_json() {
        let path = temp_config_path("round-trip");
        let mut config = HostConfig::new("hash");
        config.trust_device(TrustedDevice {
            device_id: "tablet".to_string(),
            device_name: "Tablet".to_string(),
            public_key: "public-key".to_string(),
        });

        save_config(&path, &config).expect("config saves");
        let loaded = load_config(&path, "unused").expect("config loads");

        assert_eq!(loaded, config);
        let _ = fs::remove_file(path);
    }

    fn temp_config_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("remote-poe-{name}-config.json"))
    }
}
