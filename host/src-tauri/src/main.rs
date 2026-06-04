use host_core::config::HostConfig;
use host_core::config_store::{load_config, save_config};
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
use std::path::PathBuf;
use std::process::Command;
use std::sync::{Arc, Mutex, mpsc};
use std::thread::{self, JoinHandle};
use std::time::Duration;
use tauri::{
    Manager, WindowEvent,
    menu::MenuBuilder,
    tray::TrayIconBuilder,
};

const DEFAULT_PAIRING_PASSWORD_HASH: &str =
    "c53fb561532b1638f6ce48c7992eb69eda7780a3af1b0205d40342b922a77c19";
const WINDOWS_RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
const WINDOWS_RUN_VALUE: &str = "RemotePoeHost";

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
    autostart: bool,
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

#[derive(serde::Deserialize)]
struct PairingPasswordDto {
    #[serde(rename = "passwordHash")]
    password_hash: String,
}

#[derive(serde::Deserialize)]
struct StreamConfigDto {
    width: u32,
    height: u32,
    fps: u32,
    #[serde(rename = "bitrateKbps")]
    bitrate_kbps: u32,
}

#[derive(serde::Deserialize)]
struct AutostartDto {
    enabled: bool,
}

struct AppState {
    config_path: PathBuf,
    config: SharedHostConfig,
    event_log: SharedEventLog,
    recording_input_injector: Arc<RecordingInputInjector>,
    input_injector: SharedInputInjector,
    peer_state: SharedPeerSignalingState,
    peer_gateway: SharedWebRtcPeerGateway,
    media_pipeline: Arc<Mutex<MediaPipeline>>,
    media_runtime: Mutex<Option<MediaRuntime>>,
    signaling: Mutex<Option<SignalingRuntime>>,
}

struct MediaRuntime {
    shutdown: Option<mpsc::Sender<()>>,
    join_handle: Option<JoinHandle<()>>,
}

impl MediaRuntime {
    fn stop(&mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }

        if let Some(join_handle) = self.join_handle.take() {
            let _ = join_handle.join();
        }
    }
}

impl Drop for MediaRuntime {
    fn drop(&mut self) {
        self.stop();
    }
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
        autostart: config.autostart,
    }
}

#[tauri::command]
fn start_streaming(state: tauri::State<'_, AppState>) -> Result<HostStatus, String> {
    let mut media_runtime = state
        .media_runtime
        .lock()
        .expect("media runtime lock poisoned");
    if media_runtime.is_none() {
        *media_runtime = Some(spawn_media_runtime(
            Arc::clone(&state.media_pipeline),
            Arc::clone(&state.peer_gateway),
        )?);
    }
    drop(media_runtime);

    Ok(host_status(state))
}

#[tauri::command]
fn stop_streaming(state: tauri::State<'_, AppState>) -> HostStatus {
    let mut media_runtime = state
        .media_runtime
        .lock()
        .expect("media runtime lock poisoned");
    if let Some(mut runtime) = media_runtime.take() {
        runtime.stop();
    }
    drop(media_runtime);

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
        )
        .with_config_persist_hook(config_persist_hook(
            state.config_path.clone(),
            Arc::clone(&state.media_pipeline),
        ));
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
fn update_pairing_password(
    state: tauri::State<'_, AppState>,
    request: PairingPasswordDto,
) -> Result<HostStatus, String> {
    if !is_sha256_hex(&request.password_hash) {
        return Err("passwordHash must be a SHA-256 hex string".to_string());
    }

    let mut config = state.config.lock().expect("config lock poisoned");
    config.pairing_password_hash = request.password_hash;
    save_config(&state.config_path, &config)
        .map_err(|error| format!("failed to save config: {error:?}"))?;
    drop(config);

    Ok(host_status(state))
}

#[tauri::command]
fn update_stream_config(
    state: tauri::State<'_, AppState>,
    request: StreamConfigDto,
) -> Result<HostStatus, String> {
    let next_stream = StreamConfig {
        width: request.width,
        height: request.height,
        fps: request.fps,
        bitrate_kbps: request.bitrate_kbps,
        codec: host_core::stream::VideoCodec::H264,
        display_id: None,
    };
    if !next_stream.is_supported_v1() {
        return Err("unsupported stream config".to_string());
    }

    {
        let mut config = state.config.lock().expect("config lock poisoned");
        config.stream = next_stream.clone();
        save_config(&state.config_path, &config)
            .map_err(|error| format!("failed to save config: {error:?}"))?;
    }
    state
        .media_pipeline
        .lock()
        .expect("media pipeline lock poisoned")
        .set_config(next_stream)
        .map_err(|error| format!("failed to update media pipeline: {error:?}"))?;

    Ok(host_status(state))
}

#[tauri::command]
fn update_autostart(
    state: tauri::State<'_, AppState>,
    request: AutostartDto,
) -> Result<HostStatus, String> {
    apply_login_autostart(request.enabled)?;

    let mut config = state.config.lock().expect("config lock poisoned");
    config.autostart = request.enabled;
    save_config(&state.config_path, &config)
        .map_err(|error| format!("failed to save config: {error:?}"))?;
    drop(config);

    Ok(host_status(state))
}

fn spawn_media_runtime(
    media_pipeline: Arc<Mutex<MediaPipeline>>,
    peer_gateway: SharedWebRtcPeerGateway,
) -> Result<MediaRuntime, String> {
    let (shutdown_tx, shutdown_rx) = mpsc::channel();
    let join_handle = thread::Builder::new()
        .name("remote-poe-media".to_string())
        .spawn(move || {
            if let Ok(mut pipeline) = media_pipeline.lock() {
                pipeline.start();
            }

            loop {
                let frame_interval = media_frame_interval(&media_pipeline);
                let encoded_frame = media_pipeline
                    .lock()
                    .ok()
                    .and_then(|mut pipeline| pipeline.capture_and_encode_once().ok());

                if let Some(frame) = encoded_frame {
                    let _ = peer_gateway.push_encoded_frame(&frame);
                }

                match shutdown_rx.recv_timeout(frame_interval) {
                    Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                }
            }

            if let Ok(mut pipeline) = media_pipeline.lock() {
                pipeline.stop();
            }
        })
        .map_err(|error| format!("failed to start media thread: {error}"))?;

    Ok(MediaRuntime {
        shutdown: Some(shutdown_tx),
        join_handle: Some(join_handle),
    })
}

fn media_frame_interval(media_pipeline: &Arc<Mutex<MediaPipeline>>) -> Duration {
    let fps = media_pipeline
        .lock()
        .map(|pipeline| pipeline.config().fps.max(1))
        .unwrap_or(60);
    Duration::from_millis((1000 / fps.max(1)) as u64)
}

fn host_config_path() -> PathBuf {
    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("remote-poe-host-config.json")
}

fn is_sha256_hex(value: &str) -> bool {
    value.len() == 64 && value.chars().all(|ch| ch.is_ascii_hexdigit())
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
            save_config(&state.config_path, &config)
                .map_err(|error| format!("failed to save config: {error:?}"))?;
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
    let revoked = config.revoke_device(&device_id);
    if revoked {
        let _ = save_config(&state.config_path, &config);
    }
    revoked
}

fn main() {
    let recording_input_injector = Arc::new(RecordingInputInjector::new(64));
    let input_injector = build_input_injector(Arc::clone(&recording_input_injector));
    let peer_state = Arc::new(Mutex::new(PeerSignalingState::default()));
    let config_path = host_config_path();
    let config =
        load_config(&config_path, DEFAULT_PAIRING_PASSWORD_HASH).unwrap_or_else(|_| HostConfig {
            pairing_password_hash: DEFAULT_PAIRING_PASSWORD_HASH.to_string(),
            trusted_devices: Vec::new(),
            stream: StreamConfig::default(),
            autostart: false,
        });
    let media_pipeline = Arc::new(Mutex::new(build_media_pipeline(config.stream.clone())));
    let should_autostart_services = config.autostart;
    let config = Arc::new(Mutex::new(config));
    let event_log = Arc::new(Mutex::new(SignalingEventLog::new(64)));
    let peer_gateway = build_peer_gateway(
        Arc::clone(&config),
        Arc::clone(&event_log),
        Arc::clone(&input_injector),
        Arc::clone(&peer_state),
    );

    tauri::Builder::default()
        .manage(AppState {
            config_path,
            config,
            event_log,
            recording_input_injector,
            input_injector,
            peer_state,
            peer_gateway,
            media_pipeline,
            media_runtime: Mutex::new(None),
            signaling: Mutex::new(None),
        })
        .setup(move |app| {
            setup_tray(app.handle())?;
            if should_autostart_services {
                if let Some(state) = app.try_state::<AppState>() {
                    let _ = start_background_services(&state);
                }
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            host_status,
            start_streaming,
            stop_streaming,
            start_signaling,
            stop_signaling,
            update_pairing_password,
            update_stream_config,
            update_autostart,
            pair_device,
            trusted_devices,
            revoke_device
        ])
        .run(tauri::generate_context!())
        .expect("failed to run remote POE host");
}

fn setup_tray<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> tauri::Result<()> {
    let menu = MenuBuilder::new(app)
        .text("show", "開啟 Host")
        .text("hide", "隱藏到系統匣")
        .separator()
        .text("quit", "結束")
        .build()?;

    let mut tray = TrayIconBuilder::with_id("remote-poe-host")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .tooltip("遠端 POE Host");

    if let Some(icon) = app.default_window_icon().cloned() {
        tray = tray.icon(icon);
    }

    tray.on_menu_event(|app, event| match event.id().as_ref() {
        "show" => {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }
        "hide" => {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.hide();
            }
        }
        "quit" => app.exit(0),
        _ => {}
    })
    .build(app)?;

    Ok(())
}

fn config_persist_hook(
    config_path: PathBuf,
    media_pipeline: Arc<Mutex<MediaPipeline>>,
) -> impl Fn(&HostConfig) + Send + Sync + 'static {
    move |config| {
        let _ = save_config(&config_path, config);
        if let Ok(mut pipeline) = media_pipeline.lock() {
            let _ = pipeline.set_config(config.stream.clone());
        }
    }
}

fn start_background_services(state: &tauri::State<'_, AppState>) -> Result<(), String> {
    let _ = start_signaling(state.clone())?;
    let _ = start_streaming(state.clone())?;
    Ok(())
}

#[cfg(windows)]
fn apply_login_autostart(enabled: bool) -> Result<(), String> {
    if enabled {
        let exe_path = std::env::current_exe()
            .map_err(|error| format!("failed to locate host executable: {error}"))?;
        let command = format!("\"{}\"", exe_path.display());
        run_reg_command([
            "add",
            WINDOWS_RUN_KEY,
            "/v",
            WINDOWS_RUN_VALUE,
            "/t",
            "REG_SZ",
            "/d",
            &command,
            "/f",
        ])
    } else {
        run_reg_command(["delete", WINDOWS_RUN_KEY, "/v", WINDOWS_RUN_VALUE, "/f"])
    }
}

#[cfg(not(windows))]
fn apply_login_autostart(_enabled: bool) -> Result<(), String> {
    Ok(())
}

#[cfg(windows)]
fn run_reg_command<const N: usize>(args: [&str; N]) -> Result<(), String> {
    let output = Command::new("reg")
        .args(args)
        .output()
        .map_err(|error| format!("failed to run reg.exe: {error}"))?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(format!("failed to update Windows startup setting: {stderr}"))
    }
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
fn build_peer_gateway(
    config: SharedHostConfig,
    event_log: SharedEventLog,
    input_injector: SharedInputInjector,
    peer_state: SharedPeerSignalingState,
) -> SharedWebRtcPeerGateway {
    let control_server = SignalingServer::from_shared_parts_with_input_peer_state_and_gateway(
        config,
        event_log,
        input_injector,
        peer_state,
        Arc::new(signaling_server::RecordingWebRtcPeerGateway::new()),
    );
    Arc::new(
        RealWebRtcPeerGateway::new_with_control_handler(Some(Arc::new(move |text| {
            let _ = control_server.handle_control_message(&text);
        })))
        .expect("failed to initialize WebRTC peer gateway"),
    )
}

#[cfg(not(feature = "real-webrtc"))]
fn build_peer_gateway(
    _config: SharedHostConfig,
    _event_log: SharedEventLog,
    _input_injector: SharedInputInjector,
    _peer_state: SharedPeerSignalingState,
) -> SharedWebRtcPeerGateway {
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

fn build_media_pipeline(config: StreamConfig) -> MediaPipeline {
    MediaPipeline::new(
        config,
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
