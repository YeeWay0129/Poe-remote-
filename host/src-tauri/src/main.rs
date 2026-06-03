use host_core::config::HostConfig;
use host_core::pairing::{PairingDecision, PairingRequest, evaluate_pairing};
use host_core::stream::StreamConfig;
#[cfg(windows)]
use signaling_server::windows_input::WindowsSendInputInjector;
use signaling_server::{
    CompositeInputInjector, RecordingInputInjector, SharedEventLog, SharedHostConfig,
    SharedInputInjector, SignalingBindConfig, SignalingEventLog, SignalingRuntime, SignalingServer,
    spawn_plain_ws_server,
};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

#[derive(serde::Serialize)]
struct HostStatus {
    streaming: bool,
    #[serde(rename = "signalingRunning")]
    signaling_running: bool,
    #[serde(rename = "signalingEndpoint")]
    signaling_endpoint: Option<String>,
    #[serde(rename = "signalingEvents")]
    signaling_events: Vec<String>,
    #[serde(rename = "inputEvents")]
    input_events: Vec<String>,
    #[serde(rename = "inputBackend")]
    input_backend: &'static str,
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
    config: SharedHostConfig,
    event_log: SharedEventLog,
    recording_input_injector: Arc<RecordingInputInjector>,
    input_injector: SharedInputInjector,
    streaming: Mutex<bool>,
    signaling: Mutex<Option<SignalingRuntime>>,
}

#[tauri::command]
fn host_status(state: tauri::State<'_, AppState>) -> HostStatus {
    let config = state.config.lock().expect("config lock poisoned");
    let streaming = *state.streaming.lock().expect("streaming lock poisoned");
    let signaling = state.signaling.lock().expect("signaling lock poisoned");
    let signaling_events = state
        .event_log
        .lock()
        .expect("event log lock poisoned")
        .snapshot();
    let input_events = state.recording_input_injector.snapshot();

    HostStatus {
        streaming,
        signaling_running: signaling.is_some(),
        signaling_endpoint: signaling
            .as_ref()
            .map(|runtime| format!("ws://{}/signaling", runtime.bind_addr())),
        signaling_events,
        input_events,
        input_backend: input_backend_label(),
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
fn start_signaling(state: tauri::State<'_, AppState>) -> Result<HostStatus, String> {
    let mut signaling = state.signaling.lock().expect("signaling lock poisoned");
    if signaling.is_none() {
        let server = SignalingServer::from_shared_parts_with_input(
            Arc::clone(&state.config),
            Arc::clone(&state.event_log),
            Arc::clone(&state.input_injector),
        );
        let runtime = spawn_plain_ws_server(
            server,
            SignalingBindConfig {
                bind_addr: SocketAddr::from(([0, 0, 0, 0], 7443)),
                ..SignalingBindConfig::default()
            },
        )
        .map_err(|error| format!("failed to start signaling: {error}"))?;

        *signaling = Some(runtime);
    }
    drop(signaling);

    Ok(host_status(state))
}

#[tauri::command]
fn stop_signaling(state: tauri::State<'_, AppState>) -> HostStatus {
    let mut signaling = state.signaling.lock().expect("signaling lock poisoned");
    if let Some(mut runtime) = signaling.take() {
        runtime.stop();
    }
    drop(signaling);

    host_status(state)
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
    let recording_input_injector = Arc::new(RecordingInputInjector::new(64));
    let input_injector = build_input_injector(Arc::clone(&recording_input_injector));

    tauri::Builder::default()
        .manage(AppState {
            config: Arc::new(Mutex::new(HostConfig {
                pairing_password_hash: String::new(),
                trusted_devices: Vec::new(),
                stream: StreamConfig::default(),
                autostart: false,
            })),
            event_log: Arc::new(Mutex::new(SignalingEventLog::new(64))),
            recording_input_injector,
            input_injector,
            streaming: Mutex::new(false),
            signaling: Mutex::new(None),
        })
        .invoke_handler(tauri::generate_handler![
            host_status,
            start_streaming,
            stop_streaming,
            start_signaling,
            stop_signaling,
            pair_device,
            trusted_devices,
            revoke_device
        ])
        .run(tauri::generate_context!())
        .expect("failed to run remote POE host");
}

#[cfg(windows)]
fn build_input_injector(recording: Arc<RecordingInputInjector>) -> SharedInputInjector {
    Arc::new(CompositeInputInjector::new(vec![
        Arc::new(WindowsSendInputInjector::default()),
        recording,
    ]))
}

#[cfg(not(windows))]
fn build_input_injector(recording: Arc<RecordingInputInjector>) -> SharedInputInjector {
    recording
}

#[cfg(windows)]
fn input_backend_label() -> &'static str {
    "Windows SendInput gated to POE foreground + recording"
}

#[cfg(not(windows))]
fn input_backend_label() -> &'static str {
    "recording only"
}
