use host_core::config::HostConfig;
use host_core::pairing::{evaluate_pairing, PairingDecision, PairingRequest};
use host_core::stream::StreamConfig;
use std::sync::Mutex;

#[derive(serde::Serialize)]
struct HostStatus {
    streaming: bool,
    #[serde(rename = "trustedDevices")]
    trusted_devices: usize,
    #[serde(rename = "streamLabel")]
    stream_label: String,
}

#[derive(serde::Deserialize)]
struct PairingRequestDto {
    #[serde(rename = "deviceId")]
    device_id: String,
    #[serde(rename = "deviceName")]
    device_name: String,
    #[serde(rename = "publicKey")]
    public_key: String,
    #[serde(rename = "passwordHash")]
    password_hash: String,
}

#[derive(serde::Serialize)]
struct TrustedDeviceDto {
    #[serde(rename = "deviceId")]
    device_id: String,
    #[serde(rename = "deviceName")]
    device_name: String,
    #[serde(rename = "publicKey")]
    public_key: String,
}

struct AppState {
    config: Mutex<HostConfig>,
    streaming: Mutex<bool>,
}

#[tauri::command]
fn host_status(state: tauri::State<'_, AppState>) -> HostStatus {
    let config = state.config.lock().expect("config lock poisoned");
    let streaming = *state.streaming.lock().expect("streaming lock poisoned");

    HostStatus {
        streaming,
        trusted_devices: config.trusted_devices.len(),
        stream_label: format!(
            "{}x{}@{} {}kbps",
            config.stream.width,
            config.stream.height,
            config.stream.fps,
            config.stream.bitrate_kbps
        ),
    }
}

#[tauri::command]
fn start_streaming(state: tauri::State<'_, AppState>) {
    let mut streaming = state.streaming.lock().expect("streaming lock poisoned");
    *streaming = true;
}

#[tauri::command]
fn stop_streaming(state: tauri::State<'_, AppState>) {
    let mut streaming = state.streaming.lock().expect("streaming lock poisoned");
    *streaming = false;
}

#[tauri::command]
fn pair_device(
    state: tauri::State<'_, AppState>,
    request: PairingRequestDto,
) -> Result<TrustedDeviceDto, String> {
    let mut config = state.config.lock().expect("config lock poisoned");
    let decision = evaluate_pairing(
        &config.pairing_password_hash,
        PairingRequest {
            device_id: request.device_id,
            device_name: request.device_name,
            public_key: request.public_key,
            password_hash: request.password_hash,
        },
    );

    match decision {
        PairingDecision::Trusted(device) => {
            config.trust_device(device.clone());
            Ok(TrustedDeviceDto {
                device_id: device.device_id,
                device_name: device.device_name,
                public_key: device.public_key,
            })
        }
        PairingDecision::Rejected(reason) => Err(format!("pairing rejected: {reason:?}")),
    }
}

#[tauri::command]
fn trusted_devices(state: tauri::State<'_, AppState>) -> Vec<TrustedDeviceDto> {
    let config = state.config.lock().expect("config lock poisoned");
    config
        .trusted_devices
        .iter()
        .map(|device| TrustedDeviceDto {
            device_id: device.device_id.clone(),
            device_name: device.device_name.clone(),
            public_key: device.public_key.clone(),
        })
        .collect()
}

#[tauri::command]
fn revoke_device(state: tauri::State<'_, AppState>, device_id: String) -> bool {
    let mut config = state.config.lock().expect("config lock poisoned");
    config.revoke_device(&device_id)
}

fn main() {
    tauri::Builder::default()
        .manage(AppState {
            config: Mutex::new(HostConfig {
                pairing_password_hash: String::new(),
                trusted_devices: Vec::new(),
                stream: StreamConfig::default(),
                autostart: false,
            }),
            streaming: Mutex::new(false),
        })
        .invoke_handler(tauri::generate_handler![
            host_status,
            start_streaming,
            stop_streaming,
            pair_device,
            trusted_devices,
            revoke_device
        ])
        .run(tauri::generate_context!())
        .expect("failed to run remote POE host");
}
