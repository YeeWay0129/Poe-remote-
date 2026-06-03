use host_core::config::HostConfig;
use host_core::pairing::{PairingDecision, PairingRequest, evaluate_pairing};
use host_core::stream::StreamConfig;
#[cfg(not(all(feature = "media-windows-mf-h264", windows)))]
use media_pipeline::NullH264Encoder;
#[cfg(not(all(feature = "media-windows-capture", windows)))]
use media_pipeline::RecordingFrameSource;
#[cfg(all(feature = "media-windows-capture", windows))]
use media_pipeline::WindowsGdiFrameSource;
#[cfg(all(feature = "media-windows-mf-h264", windows))]
use media_pipeline::WindowsMediaFoundationH264Encoder;
use media_pipeline::{FrameSource, MediaPipeline, VideoEncoder};
#[cfg(feature = "real-webrtc")]
use signaling_server::RealWebRtcPeerGateway;
#[cfg(windows)]
use signaling_server::windows_input::WindowsSendInputInjector;
use signaling_server::{
    CompositeInputInjector, PeerSignalingState, RecordingInputInjector, SharedEventLog,
    SharedHostConfig, SharedInputInjector, SharedPeerSignalingState, SharedWebRtcPeerGateway,
    SignalingBindConfig, SignalingEventLog, SignalingRuntime, SignalingServer,
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
    #[serde(rename = "peerBackend")]
    peer_backend: &'static str,
    #[serde(rename = "mediaBackend")]
    media_backend: &'static str,
    #[serde(rename = "capturedFrames")]
    captured_frames: u64,
    #[serde(rename = "encodedFrames")]
    encoded_frames: u64,
    #[serde(rename = "lastCapturedBytes")]
    last_captured_bytes: Option<usize>,
    #[serde(rename = "lastEncodedBytes")]
    last_encoded_bytes: Option<usize>,
    #[serde(rename = "trustedDevices")]
    trusted_devices: usize,
    #[serde(rename = "streamLabel")]
    stream_label: String,
    #[serde(rename = "peerPhase")]
    peer_phase: &'static str,
    #[serde(rename = "lastOfferBytes")]
    last_offer_bytes: Option<usize>,
    #[serde(rename = "lastAnswerBytes")]
    last_answer_bytes: Option<usize>,
    #[serde(rename = "iceCandidates")]
    ice_candidates: usize,
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
    peer_state: SharedPeerSignalingState,
    peer_gateway: SharedWebRtcPeerGateway,
    media_pipeline: Mutex<MediaPipeline>,
    signaling: Mutex<Option<SignalingRuntime>>,
}

#[tauri::command]
fn host_status(state: tauri::State<'_, AppState>) -> HostStatus {
    let config = state.config.lock().expect("config lock poisoned");
    let media_pipeline = state
        .media_pipeline
        .lock()
        .expect("media pipeline lock poisoned");
    let media_stats = media_pipeline.stats();
    let signaling = state.signaling.lock().expect("signaling lock poisoned");
    let signaling_events = state
        .event_log
        .lock()
        .expect("event log lock poisoned")
        .snapshot();
    let input_events = state.recording_input_injector.snapshot();
    let peer_state = state
        .peer_state
        .lock()
        .expect("peer signaling lock poisoned")
        .clone();

    HostStatus {
        streaming: media_pipeline.is_running(),
        signaling_running: signaling.is_some(),
        signaling_endpoint: signaling
            .as_ref()
            .map(|runtime| format!("ws://{}/signaling", runtime.bind_addr())),
        signaling_events,
        input_events,
        input_backend: input_backend_label(),
        peer_backend: peer_backend_label(),
        media_backend: media_backend_label(),
        captured_frames: media_stats.captured_frames,
        encoded_frames: media_stats.encoded_frames,
        last_captured_bytes: media_stats.last_captured_bytes,
        last_encoded_bytes: media_stats.last_encoded_bytes,
        trusted_devices: config.trusted_devices.len(),
        stream_label: format!(
            "{}x{}@{} {}kbps",
            config.stream.width,
            config.stream.height,
            config.stream.fps,
            config.stream.bitrate_kbps
        ),
        peer_phase: peer_state.phase.as_str(),
        last_offer_bytes: peer_state.last_offer_sdp_bytes,
        last_answer_bytes: peer_state.last_answer_sdp_bytes,
        ice_candidates: peer_state.received_ice_candidates,
    }
}

#[tauri::command]
fn start_streaming(state: tauri::State<'_, AppState>) -> Result<HostStatus, String> {
    let mut media_pipeline = state
        .media_pipeline
        .lock()
        .expect("media pipeline lock poisoned");
    media_pipeline.start();
    let encoded_frame = media_pipeline
        .capture_and_encode_once()
        .map_err(|error| format!("failed to capture first frame: {error:?}"))?;
    drop(media_pipeline);
    state
        .peer_gateway
        .push_encoded_frame(&encoded_frame)
        .map_err(|error| format!("failed to queue encoded frame: {error:?}"))?;

    Ok(host_status(state))
}

#[tauri::command]
fn stop_streaming(state: tauri::State<'_, AppState>) -> HostStatus {
    let mut media_pipeline = state
        .media_pipeline
        .lock()
        .expect("media pipeline lock poisoned");
    media_pipeline.stop();
    drop(media_pipeline);

    host_status(state)
}

#[tauri::command]
fn start_signaling(state: tauri::State<'_, AppState>) -> Result<HostStatus, String> {
    let mut signaling = state.signaling.lock().expect("signaling lock poisoned");
    if signaling.is_none() {
        let server = SignalingServer::from_shared_parts_with_input_peer_state_and_gateway(
            Arc::clone(&state.config),
            Arc::clone(&state.event_log),
            Arc::clone(&state.input_injector),
            Arc::clone(&state.peer_state),
            Arc::clone(&state.peer_gateway),
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
    let peer_state = Arc::new(Mutex::new(PeerSignalingState::default()));
    let peer_gateway = build_peer_gateway();
    let media_pipeline = build_media_pipeline();

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
            peer_state,
            peer_gateway,
            media_pipeline: Mutex::new(media_pipeline),
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

#[cfg(feature = "real-webrtc")]
fn build_peer_gateway() -> SharedWebRtcPeerGateway {
    Arc::new(RealWebRtcPeerGateway::new().expect("failed to initialize WebRTC peer gateway"))
}

#[cfg(not(feature = "real-webrtc"))]
fn build_peer_gateway() -> SharedWebRtcPeerGateway {
    Arc::new(signaling_server::RecordingWebRtcPeerGateway::new())
}

#[cfg(feature = "real-webrtc")]
fn peer_backend_label() -> &'static str {
    "real webrtc"
}

#[cfg(not(feature = "real-webrtc"))]
fn peer_backend_label() -> &'static str {
    "recording webrtc"
}

fn build_media_pipeline() -> MediaPipeline {
    MediaPipeline::new(
        StreamConfig::default(),
        build_frame_source(),
        build_video_encoder(),
    )
    .expect("default media pipeline config must be supported")
}

#[cfg(all(feature = "media-windows-capture", windows))]
fn build_frame_source() -> Box<dyn FrameSource> {
    Box::new(WindowsGdiFrameSource::default())
}

#[cfg(not(all(feature = "media-windows-capture", windows)))]
fn build_frame_source() -> Box<dyn FrameSource> {
    Box::new(RecordingFrameSource::default())
}

#[cfg(all(feature = "media-windows-mf-h264", windows))]
fn build_video_encoder() -> Box<dyn VideoEncoder> {
    Box::new(
        WindowsMediaFoundationH264Encoder::new()
            .expect("failed to initialize Media Foundation H.264 encoder"),
    )
}

#[cfg(not(all(feature = "media-windows-mf-h264", windows)))]
fn build_video_encoder() -> Box<dyn VideoEncoder> {
    Box::new(NullH264Encoder::default())
}

#[cfg(all(
    feature = "media-windows-capture",
    feature = "media-windows-mf-h264",
    windows
))]
fn media_backend_label() -> &'static str {
    "Windows GDI capture + Media Foundation h264"
}

#[cfg(all(
    feature = "media-windows-capture",
    not(feature = "media-windows-mf-h264"),
    windows
))]
fn media_backend_label() -> &'static str {
    "Windows GDI capture + null h264 encoder"
}

#[cfg(all(
    not(feature = "media-windows-capture"),
    feature = "media-windows-mf-h264",
    windows
))]
fn media_backend_label() -> &'static str {
    "recording capture + Media Foundation h264"
}

#[cfg(not(any(
    all(feature = "media-windows-capture", windows),
    all(feature = "media-windows-mf-h264", windows)
)))]
fn media_backend_label() -> &'static str {
    "recording capture + null h264 encoder"
}
