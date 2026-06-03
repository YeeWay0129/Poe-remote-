use futures_util::{SinkExt, StreamExt};
use host_core::config::HostConfig;
use host_core::input::{
    ButtonAction, InputEvent, KeyAction, MouseButton, PointerMode, validate_user_event,
};
use host_core::pairing::{PairingDecision, PairingRequest, evaluate_pairing, is_trusted_device};
use host_core::signaling::{
    AuthPayload, ErrorPayload, SignalingMessage, SignalingPayload, SignalingType,
};
use std::collections::VecDeque;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::oneshot;
use tokio_tungstenite::accept_async;
use tokio_tungstenite::tungstenite::Message;

pub type SharedHostConfig = Arc<Mutex<HostConfig>>;
pub type SharedEventLog = Arc<Mutex<SignalingEventLog>>;
pub type SharedInputInjector = Arc<dyn InputInjector>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignalingEventLog {
    max_len: usize,
    records: VecDeque<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputInjectionError {
    BackendUnavailable,
    UnsafeForegroundWindow,
}

pub trait InputInjector: Send + Sync {
    fn inject(&self, event: &InputEvent) -> Result<(), InputInjectionError>;
}

pub struct CompositeInputInjector {
    injectors: Vec<SharedInputInjector>,
}

impl CompositeInputInjector {
    pub fn new(injectors: Vec<SharedInputInjector>) -> Self {
        Self { injectors }
    }
}

impl InputInjector for CompositeInputInjector {
    fn inject(&self, event: &InputEvent) -> Result<(), InputInjectionError> {
        for injector in &self.injectors {
            injector.inject(event)?;
        }
        Ok(())
    }
}

#[derive(Debug)]
pub struct RecordingInputInjector {
    max_len: usize,
    records: Mutex<VecDeque<String>>,
}

impl RecordingInputInjector {
    pub fn new(max_len: usize) -> Self {
        Self {
            max_len,
            records: Mutex::new(VecDeque::new()),
        }
    }

    pub fn snapshot(&self) -> Vec<String> {
        self.records
            .lock()
            .map(|records| records.iter().cloned().collect())
            .unwrap_or_default()
    }

    fn push(&self, record: impl Into<String>) {
        if self.max_len == 0 {
            return;
        }

        if let Ok(mut records) = self.records.lock() {
            while records.len() >= self.max_len {
                records.pop_front();
            }
            records.push_back(record.into());
        }
    }
}

impl InputInjector for RecordingInputInjector {
    fn inject(&self, event: &InputEvent) -> Result<(), InputInjectionError> {
        self.push(input_event_summary(event));
        Ok(())
    }
}

#[cfg(windows)]
pub mod windows_input {
    use super::{InputEvent, InputInjectionError, InputInjector};
    use host_core::input::{ButtonAction, KeyAction, MouseButton, PointerMode};
    use std::mem::size_of;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBD_EVENT_FLAGS, KEYBDINPUT,
        KEYEVENTF_KEYUP, MOUSE_EVENT_FLAGS, MOUSEEVENTF_ABSOLUTE, MOUSEEVENTF_HWHEEL,
        MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP,
        MOUSEEVENTF_MOVE, MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, MOUSEEVENTF_WHEEL,
        MOUSEEVENTF_XDOWN, MOUSEEVENTF_XUP, MOUSEINPUT, SendInput, VIRTUAL_KEY,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowTextLengthW, GetWindowTextW,
    };

    const XBUTTON1_DATA: u32 = 1;
    const XBUTTON2_DATA: u32 = 2;

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct WindowsForegroundWindowGate {
        allowed_title_fragments: Vec<String>,
    }

    impl WindowsForegroundWindowGate {
        pub fn poe_default() -> Self {
            Self {
                allowed_title_fragments: vec!["path of exile".to_string(), "poe".to_string()],
            }
        }

        pub fn new(allowed_title_fragments: Vec<String>) -> Self {
            Self {
                allowed_title_fragments: allowed_title_fragments
                    .into_iter()
                    .map(|fragment| fragment.to_ascii_lowercase())
                    .collect(),
            }
        }

        pub fn allows_title(&self, title: &str) -> bool {
            let title = title.to_ascii_lowercase();
            self.allowed_title_fragments
                .iter()
                .any(|fragment| !fragment.is_empty() && title.contains(fragment))
        }

        fn allows_foreground_window(&self) -> bool {
            foreground_window_title()
                .as_deref()
                .is_some_and(|title| self.allows_title(title))
        }
    }

    #[derive(Debug)]
    pub struct WindowsSendInputInjector {
        foreground_gate: WindowsForegroundWindowGate,
    }

    impl Default for WindowsSendInputInjector {
        fn default() -> Self {
            Self {
                foreground_gate: WindowsForegroundWindowGate::poe_default(),
            }
        }
    }

    impl WindowsSendInputInjector {
        pub fn new(foreground_gate: WindowsForegroundWindowGate) -> Self {
            Self { foreground_gate }
        }
    }

    impl InputInjector for WindowsSendInputInjector {
        fn inject(&self, event: &InputEvent) -> Result<(), InputInjectionError> {
            if !self.foreground_gate.allows_foreground_window() {
                return Err(InputInjectionError::UnsafeForegroundWindow);
            }

            let mut inputs = event_to_windows_inputs(event);
            if inputs.is_empty() {
                return Ok(());
            }

            let sent = unsafe {
                SendInput(
                    inputs.len() as u32,
                    inputs.as_mut_ptr(),
                    size_of::<INPUT>() as i32,
                )
            };

            if sent == inputs.len() as u32 {
                Ok(())
            } else {
                Err(InputInjectionError::BackendUnavailable)
            }
        }
    }

    pub fn event_to_windows_inputs(event: &InputEvent) -> Vec<INPUT> {
        match event {
            InputEvent::Keyboard { key_code, action } => {
                vec![keyboard_input(
                    *key_code as VIRTUAL_KEY,
                    key_action_flags(action),
                )]
            }
            InputEvent::MouseMove { dx, dy, mode } => {
                vec![mouse_input(
                    *dx,
                    *dy,
                    0,
                    match mode {
                        PointerMode::Relative => MOUSEEVENTF_MOVE,
                        PointerMode::Absolute => MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE,
                    },
                )]
            }
            InputEvent::MouseButton { button, action } => {
                vec![mouse_input(
                    0,
                    0,
                    mouse_button_data(button),
                    mouse_button_flags(button, action),
                )]
            }
            InputEvent::MouseWheel { delta_x, delta_y } => {
                let mut inputs = Vec::new();
                if *delta_y != 0 {
                    inputs.push(mouse_input(0, 0, *delta_y as u32, MOUSEEVENTF_WHEEL));
                }
                if *delta_x != 0 {
                    inputs.push(mouse_input(0, 0, *delta_x as u32, MOUSEEVENTF_HWHEEL));
                }
                inputs
            }
            InputEvent::PointerModeChanged(_) => Vec::new(),
        }
    }

    fn keyboard_input(vk: VIRTUAL_KEY, flags: KEYBD_EVENT_FLAGS) -> INPUT {
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: vk,
                    wScan: 0,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        }
    }

    fn mouse_input(dx: i32, dy: i32, mouse_data: u32, flags: MOUSE_EVENT_FLAGS) -> INPUT {
        INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT {
                    dx,
                    dy,
                    mouseData: mouse_data,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        }
    }

    fn key_action_flags(action: &KeyAction) -> KEYBD_EVENT_FLAGS {
        match action {
            KeyAction::Down => 0,
            KeyAction::Up => KEYEVENTF_KEYUP,
        }
    }

    fn mouse_button_flags(button: &MouseButton, action: &ButtonAction) -> MOUSE_EVENT_FLAGS {
        match (button, action) {
            (MouseButton::Left, ButtonAction::Down) => MOUSEEVENTF_LEFTDOWN,
            (MouseButton::Left, ButtonAction::Up) => MOUSEEVENTF_LEFTUP,
            (MouseButton::Right, ButtonAction::Down) => MOUSEEVENTF_RIGHTDOWN,
            (MouseButton::Right, ButtonAction::Up) => MOUSEEVENTF_RIGHTUP,
            (MouseButton::Middle, ButtonAction::Down) => MOUSEEVENTF_MIDDLEDOWN,
            (MouseButton::Middle, ButtonAction::Up) => MOUSEEVENTF_MIDDLEUP,
            (MouseButton::Back | MouseButton::Forward, ButtonAction::Down) => MOUSEEVENTF_XDOWN,
            (MouseButton::Back | MouseButton::Forward, ButtonAction::Up) => MOUSEEVENTF_XUP,
        }
    }

    fn mouse_button_data(button: &MouseButton) -> u32 {
        match button {
            MouseButton::Back => XBUTTON1_DATA,
            MouseButton::Forward => XBUTTON2_DATA,
            _ => 0,
        }
    }

    fn foreground_window_title() -> Option<String> {
        let hwnd = unsafe { GetForegroundWindow() };
        if hwnd.is_null() {
            return None;
        }

        let len = unsafe { GetWindowTextLengthW(hwnd) };
        if len <= 0 {
            return None;
        }

        let mut buffer = vec![0u16; len as usize + 1];
        let copied = unsafe { GetWindowTextW(hwnd, buffer.as_mut_ptr(), buffer.len() as i32) };
        if copied <= 0 {
            return None;
        }

        Some(String::from_utf16_lossy(&buffer[..copied as usize]))
    }
}

impl SignalingEventLog {
    pub fn new(max_len: usize) -> Self {
        Self {
            max_len,
            records: VecDeque::new(),
        }
    }

    pub fn push(&mut self, record: impl Into<String>) {
        if self.max_len == 0 {
            return;
        }

        while self.records.len() >= self.max_len {
            self.records.pop_front();
        }
        self.records.push_back(record.into());
    }

    pub fn snapshot(&self) -> Vec<String> {
        self.records.iter().cloned().collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignalingBindConfig {
    pub bind_addr: SocketAddr,
    pub security: TransportSecurity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportSecurity {
    PlainWsForVpn,
    TlsTerminatedUpstream,
}

impl Default for SignalingBindConfig {
    fn default() -> Self {
        Self {
            bind_addr: SocketAddr::from(([0, 0, 0, 0], 7443)),
            security: TransportSecurity::PlainWsForVpn,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerEvent {
    DeviceTrusted { device_id: String },
    DeviceAuthenticated { device_id: String },
    StreamConfigUpdated { width: u32, height: u32, fps: u32 },
    InputInjected,
    SessionDescriptionReceived(SignalingType),
    IceCandidateReceived,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrameResult {
    Accepted(ServerEvent),
    Reply(SignalingMessage),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignalingFrameError {
    InvalidJson,
    InvalidShape,
    Unauthorized,
    InvalidInput,
    InputInjectionFailed,
    UnsupportedMessage,
    StatePoisoned,
}

#[derive(Clone)]
pub struct SignalingServer {
    state: SharedHostConfig,
    event_log: SharedEventLog,
    input_injector: SharedInputInjector,
}

impl SignalingServer {
    pub fn new(config: HostConfig) -> Self {
        Self::from_shared_parts(
            Arc::new(Mutex::new(config)),
            Arc::new(Mutex::new(SignalingEventLog::new(64))),
        )
    }

    pub fn from_shared_state(state: SharedHostConfig) -> Self {
        Self::from_shared_parts(state, Arc::new(Mutex::new(SignalingEventLog::new(64))))
    }

    pub fn from_shared_parts(state: SharedHostConfig, event_log: SharedEventLog) -> Self {
        Self::from_shared_parts_with_input(
            state,
            event_log,
            Arc::new(RecordingInputInjector::new(64)),
        )
    }

    pub fn from_shared_parts_with_input(
        state: SharedHostConfig,
        event_log: SharedEventLog,
        input_injector: SharedInputInjector,
    ) -> Self {
        Self {
            state,
            event_log,
            input_injector,
        }
    }

    pub fn snapshot_config(&self) -> Result<HostConfig, SignalingFrameError> {
        self.state
            .lock()
            .map(|config| config.clone())
            .map_err(|_| SignalingFrameError::StatePoisoned)
    }

    pub fn snapshot_recent_events(&self) -> Result<Vec<String>, SignalingFrameError> {
        self.event_log
            .lock()
            .map(|log| log.snapshot())
            .map_err(|_| SignalingFrameError::StatePoisoned)
    }

    pub fn handle_text_frame(&self, text: &str) -> Result<FrameResult, SignalingFrameError> {
        let result = self.handle_text_frame_inner(text);
        self.record_frame_result(&result);
        result
    }

    fn handle_text_frame_inner(&self, text: &str) -> Result<FrameResult, SignalingFrameError> {
        let message: SignalingMessage =
            serde_json::from_str(text).map_err(|_| SignalingFrameError::InvalidJson)?;
        message
            .validate_shape()
            .map_err(|_| SignalingFrameError::InvalidShape)?;

        match message.payload {
            SignalingPayload::Auth(payload) => self.handle_auth(message.request_id, payload),
            SignalingPayload::DeviceInfo(payload) => {
                if payload.device_id.trim().is_empty() {
                    return Err(SignalingFrameError::InvalidShape);
                }
                Ok(FrameResult::Accepted(ServerEvent::DeviceAuthenticated {
                    device_id: payload.device_id,
                }))
            }
            SignalingPayload::StreamConfig(config) => {
                if !config.is_supported_v1() {
                    return Ok(FrameResult::Reply(error_reply(
                        message.request_id,
                        "unsupported_stream_config",
                        "Only 1080p60 and 720p60 H.264 are supported in v1.",
                        true,
                    )));
                }

                let mut state = self
                    .state
                    .lock()
                    .map_err(|_| SignalingFrameError::StatePoisoned)?;
                state.stream = config.clone();
                Ok(FrameResult::Accepted(ServerEvent::StreamConfigUpdated {
                    width: config.width,
                    height: config.height,
                    fps: config.fps,
                }))
            }
            SignalingPayload::InputEvent(event) => {
                validate_user_event(&event).map_err(|_| SignalingFrameError::InvalidInput)?;
                self.input_injector
                    .inject(&event)
                    .map_err(|_| SignalingFrameError::InputInjectionFailed)?;
                Ok(FrameResult::Accepted(ServerEvent::InputInjected))
            }
            SignalingPayload::SessionDescription(_) => Ok(FrameResult::Accepted(
                ServerEvent::SessionDescriptionReceived(message.message_type),
            )),
            SignalingPayload::Ice(_) => {
                Ok(FrameResult::Accepted(ServerEvent::IceCandidateReceived))
            }
            SignalingPayload::Error(_) => Err(SignalingFrameError::UnsupportedMessage),
        }
    }

    fn record_frame_result(&self, result: &Result<FrameResult, SignalingFrameError>) {
        let record = match result {
            Ok(FrameResult::Accepted(event)) => format!("accepted: {}", event.summary()),
            Ok(FrameResult::Reply(reply)) => {
                format!(
                    "reply: {} for {}",
                    signaling_type_wire_name(&reply.message_type),
                    reply.request_id
                )
            }
            Err(error) => format!("error: {error:?}"),
        };

        if let Ok(mut event_log) = self.event_log.lock() {
            event_log.push(record);
        }
    }

    fn handle_auth(
        &self,
        request_id: String,
        payload: AuthPayload,
    ) -> Result<FrameResult, SignalingFrameError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| SignalingFrameError::StatePoisoned)?;

        if is_trusted_device(
            &state.trusted_devices,
            &payload.device_id,
            &payload.public_key,
        ) {
            return Ok(FrameResult::Accepted(ServerEvent::DeviceAuthenticated {
                device_id: payload.device_id,
            }));
        }

        let decision = evaluate_pairing(
            &state.pairing_password_hash,
            PairingRequest {
                device_id: payload.device_id,
                device_name: payload.device_name,
                public_key: payload.public_key,
                password_hash: payload.password_hash,
            },
        );

        match decision {
            PairingDecision::Trusted(device) => {
                let device_id = device.device_id.clone();
                state.trust_device(device);
                Ok(FrameResult::Accepted(ServerEvent::DeviceTrusted {
                    device_id,
                }))
            }
            PairingDecision::Rejected(reason) => Ok(FrameResult::Reply(error_reply(
                request_id,
                "auth_rejected",
                &format!("Pairing rejected: {reason:?}"),
                false,
            ))),
        }
    }
}

impl ServerEvent {
    fn summary(&self) -> String {
        match self {
            ServerEvent::DeviceTrusted { device_id } => format!("device trusted {device_id}"),
            ServerEvent::DeviceAuthenticated { device_id } => {
                format!("device authenticated {device_id}")
            }
            ServerEvent::StreamConfigUpdated { width, height, fps } => {
                format!("stream config {width}x{height}@{fps}")
            }
            ServerEvent::InputInjected => "input injected".to_string(),
            ServerEvent::SessionDescriptionReceived(kind) => {
                format!("session description {}", signaling_type_wire_name(kind))
            }
            ServerEvent::IceCandidateReceived => "ice candidate".to_string(),
        }
    }
}

fn input_event_summary(event: &InputEvent) -> String {
    match event {
        InputEvent::Keyboard { key_code, action } => {
            format!("keyboard {} {key_code}", key_action_name(action))
        }
        InputEvent::MouseMove { dx, dy, mode } => {
            format!("mouse move {} dx={dx} dy={dy}", pointer_mode_name(mode))
        }
        InputEvent::MouseButton { button, action } => {
            format!(
                "mouse button {} {}",
                mouse_button_name(button),
                button_action_name(action)
            )
        }
        InputEvent::MouseWheel { delta_x, delta_y } => {
            format!("mouse wheel dx={delta_x} dy={delta_y}")
        }
        InputEvent::PointerModeChanged(mode) => {
            format!("pointer mode {}", pointer_mode_name(mode))
        }
    }
}

fn key_action_name(action: &KeyAction) -> &'static str {
    match action {
        KeyAction::Down => "down",
        KeyAction::Up => "up",
    }
}

fn button_action_name(action: &ButtonAction) -> &'static str {
    match action {
        ButtonAction::Down => "down",
        ButtonAction::Up => "up",
    }
}

fn mouse_button_name(button: &MouseButton) -> &'static str {
    match button {
        MouseButton::Left => "left",
        MouseButton::Right => "right",
        MouseButton::Middle => "middle",
        MouseButton::Back => "back",
        MouseButton::Forward => "forward",
    }
}

fn pointer_mode_name(mode: &PointerMode) -> &'static str {
    match mode {
        PointerMode::Relative => "relative",
        PointerMode::Absolute => "absolute",
    }
}

fn signaling_type_wire_name(signaling_type: &SignalingType) -> &'static str {
    match signaling_type {
        SignalingType::Auth => "auth",
        SignalingType::DeviceInfo => "device_info",
        SignalingType::StreamConfig => "stream_config",
        SignalingType::Offer => "offer",
        SignalingType::Answer => "answer",
        SignalingType::Ice => "ice",
        SignalingType::InputEvent => "input_event",
        SignalingType::Error => "error",
    }
}

pub struct SignalingRuntime {
    bind_addr: SocketAddr,
    shutdown: Option<oneshot::Sender<()>>,
    join_handle: Option<JoinHandle<()>>,
}

impl SignalingRuntime {
    pub fn bind_addr(&self) -> SocketAddr {
        self.bind_addr
    }

    pub fn stop(&mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }

        if let Some(join_handle) = self.join_handle.take() {
            let _ = join_handle.join();
        }
    }
}

impl Drop for SignalingRuntime {
    fn drop(&mut self) {
        self.stop();
    }
}

pub fn spawn_plain_ws_server(
    server: SignalingServer,
    config: SignalingBindConfig,
) -> std::io::Result<SignalingRuntime> {
    let std_listener = std::net::TcpListener::bind(config.bind_addr)?;
    std_listener.set_nonblocking(true)?;
    let bind_addr = std_listener.local_addr()?;
    let (shutdown_tx, shutdown_rx) = oneshot::channel();

    let join_handle = thread::Builder::new()
        .name("remote-poe-signaling".to_string())
        .spawn(move || {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build();
            let Ok(runtime) = runtime else {
                return;
            };

            runtime.block_on(async move {
                if let Ok(listener) = TcpListener::from_std(std_listener) {
                    let _ = run_plain_ws_server_with_listener(server, listener, shutdown_rx).await;
                }
            });
        })?;

    Ok(SignalingRuntime {
        bind_addr,
        shutdown: Some(shutdown_tx),
        join_handle: Some(join_handle),
    })
}

pub async fn run_plain_ws_server(
    server: SignalingServer,
    config: SignalingBindConfig,
) -> std::io::Result<()> {
    let listener = TcpListener::bind(config.bind_addr).await?;
    let (_shutdown_tx, shutdown_rx) = oneshot::channel();
    run_plain_ws_server_with_listener(server, listener, shutdown_rx).await
}

async fn run_plain_ws_server_with_listener(
    server: SignalingServer,
    listener: TcpListener,
    mut shutdown_rx: oneshot::Receiver<()>,
) -> std::io::Result<()> {
    loop {
        tokio::select! {
            accept_result = listener.accept() => {
                let (stream, _) = accept_result?;
                let server = server.clone();
                tokio::spawn(async move {
                    let _ = handle_connection(server, stream).await;
                });
            }
            _ = &mut shutdown_rx => {
                break;
            }
        }
    }

    Ok(())
}

async fn handle_connection(
    server: SignalingServer,
    stream: TcpStream,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut socket = accept_async(stream).await?;

    while let Some(message) = socket.next().await {
        let message = message?;
        if let Message::Text(text) = message {
            if let Ok(FrameResult::Reply(reply)) = server.handle_text_frame(&text) {
                socket
                    .send(Message::Text(serde_json::to_string(&reply)?.into()))
                    .await?;
            }
        }
    }

    Ok(())
}

fn error_reply(
    request_id: String,
    code: &str,
    message: &str,
    recoverable: bool,
) -> SignalingMessage {
    SignalingMessage {
        message_type: SignalingType::Error,
        request_id,
        payload: SignalingPayload::Error(ErrorPayload {
            code: code.to_string(),
            message: message.to_string(),
            recoverable,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use host_core::input::{InputEvent, KeyAction};
    use host_core::pairing::TrustedDevice;
    use host_core::stream::StreamConfig;

    #[test]
    fn auth_frame_trusts_new_device_when_password_matches() {
        let server = SignalingServer::new(HostConfig::new("expected-hash"));
        let message = SignalingMessage::auth(
            "req-1",
            PairingRequest {
                device_id: "tablet-1".to_string(),
                device_name: "Tablet".to_string(),
                public_key: "key".to_string(),
                password_hash: "expected-hash".to_string(),
            },
        );
        let json = serde_json::to_string(&message).expect("message serializes");

        let result = server.handle_text_frame(&json);

        assert_eq!(
            result,
            Ok(FrameResult::Accepted(ServerEvent::DeviceTrusted {
                device_id: "tablet-1".to_string()
            }))
        );
        assert_eq!(
            server.snapshot_config().unwrap().trusted_devices[0].device_id,
            "tablet-1"
        );
    }

    #[test]
    fn auth_frame_accepts_existing_trusted_device_without_password() {
        let mut config = HostConfig::new("expected-hash");
        config.trust_device(TrustedDevice {
            device_id: "tablet-1".to_string(),
            device_name: "Tablet".to_string(),
            public_key: "key".to_string(),
        });
        let server = SignalingServer::new(config);
        let message = SignalingMessage::auth(
            "req-1",
            PairingRequest {
                device_id: "tablet-1".to_string(),
                device_name: "Tablet".to_string(),
                public_key: "key".to_string(),
                password_hash: "wrong".to_string(),
            },
        );
        let json = serde_json::to_string(&message).expect("message serializes");

        assert_eq!(
            server.handle_text_frame(&json),
            Ok(FrameResult::Accepted(ServerEvent::DeviceAuthenticated {
                device_id: "tablet-1".to_string()
            }))
        );
    }

    #[test]
    fn stream_config_frame_updates_state() {
        let server = SignalingServer::new(HostConfig::new("hash"));
        let message = SignalingMessage::stream_config("req-2", StreamConfig::fallback_720p60());
        let json = serde_json::to_string(&message).expect("message serializes");

        assert_eq!(
            server.handle_text_frame(&json),
            Ok(FrameResult::Accepted(ServerEvent::StreamConfigUpdated {
                width: 1280,
                height: 720,
                fps: 60,
            }))
        );
        assert_eq!(server.snapshot_config().unwrap().stream.width, 1280);
    }

    #[test]
    fn input_frame_rejects_noop_input() {
        let server = SignalingServer::new(HostConfig::new("hash"));
        let message = SignalingMessage::input_event(
            "req-3",
            InputEvent::MouseWheel {
                delta_x: 0,
                delta_y: 0,
            },
        );
        let json = serde_json::to_string(&message).expect("message serializes");

        assert_eq!(
            server.handle_text_frame(&json),
            Err(SignalingFrameError::InvalidInput)
        );
    }

    #[test]
    fn input_frame_accepts_keyboard_action() {
        let server = SignalingServer::new(HostConfig::new("hash"));
        let message = SignalingMessage::input_event(
            "req-4",
            InputEvent::Keyboard {
                key_code: 87,
                action: KeyAction::Down,
            },
        );
        let json = serde_json::to_string(&message).expect("message serializes");

        assert_eq!(
            server.handle_text_frame(&json),
            Ok(FrameResult::Accepted(ServerEvent::InputInjected))
        );
    }

    #[test]
    fn input_frame_records_injected_keyboard_action() {
        let injector = Arc::new(RecordingInputInjector::new(8));
        let server = SignalingServer::from_shared_parts_with_input(
            Arc::new(Mutex::new(HostConfig::new("hash"))),
            Arc::new(Mutex::new(SignalingEventLog::new(8))),
            injector.clone(),
        );
        let message = SignalingMessage::input_event(
            "req-6",
            InputEvent::Keyboard {
                key_code: 87,
                action: KeyAction::Down,
            },
        );
        let json = serde_json::to_string(&message).expect("message serializes");

        server.handle_text_frame(&json).expect("frame accepted");

        assert_eq!(injector.snapshot(), vec!["keyboard down 87".to_string()]);
    }

    #[test]
    fn composite_input_injector_invokes_recording_backend() {
        let recording = Arc::new(RecordingInputInjector::new(8));
        let composite = CompositeInputInjector::new(vec![recording.clone()]);
        let event = InputEvent::MouseButton {
            button: host_core::input::MouseButton::Left,
            action: host_core::input::ButtonAction::Down,
        };

        composite.inject(&event).expect("composite injects");

        assert_eq!(
            recording.snapshot(),
            vec!["mouse button left down".to_string()]
        );
    }

    #[cfg(windows)]
    #[test]
    fn windows_input_conversion_maps_keyboard_without_injecting() {
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::{INPUT_KEYBOARD, KEYEVENTF_KEYUP};

        let inputs = crate::windows_input::event_to_windows_inputs(&InputEvent::Keyboard {
            key_code: 87,
            action: KeyAction::Up,
        });

        assert_eq!(inputs.len(), 1);
        assert_eq!(inputs[0].r#type, INPUT_KEYBOARD);
        unsafe {
            assert_eq!(inputs[0].Anonymous.ki.wVk, 87);
            assert_eq!(inputs[0].Anonymous.ki.dwFlags, KEYEVENTF_KEYUP);
        }
    }

    #[cfg(windows)]
    #[test]
    fn windows_input_conversion_splits_wheel_axes() {
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
            INPUT_MOUSE, MOUSEEVENTF_HWHEEL, MOUSEEVENTF_WHEEL,
        };

        let inputs = crate::windows_input::event_to_windows_inputs(&InputEvent::MouseWheel {
            delta_x: 120,
            delta_y: -120,
        });

        assert_eq!(inputs.len(), 2);
        assert_eq!(inputs[0].r#type, INPUT_MOUSE);
        assert_eq!(inputs[1].r#type, INPUT_MOUSE);
        unsafe {
            assert_eq!(inputs[0].Anonymous.mi.dwFlags, MOUSEEVENTF_WHEEL);
            assert_eq!(inputs[1].Anonymous.mi.dwFlags, MOUSEEVENTF_HWHEEL);
        }
    }

    #[cfg(windows)]
    #[test]
    fn windows_foreground_gate_allows_poe_titles() {
        let gate = crate::windows_input::WindowsForegroundWindowGate::poe_default();

        assert!(gate.allows_title("Path of Exile"));
        assert!(gate.allows_title("POE tools overlay"));
        assert!(!gate.allows_title("Untitled - Notepad"));
    }

    #[cfg(windows)]
    #[test]
    fn windows_foreground_gate_uses_custom_fragments() {
        let gate = crate::windows_input::WindowsForegroundWindowGate::new(vec![
            "Path of Exile 2".to_string(),
        ]);

        assert!(gate.allows_title("Path of Exile 2"));
        assert!(!gate.allows_title("Path of Exile"));
    }

    #[test]
    fn event_log_records_recent_frame_results() {
        let server = SignalingServer::new(HostConfig::new("hash"));
        let message = SignalingMessage::stream_config("req-5", StreamConfig::fallback_720p60());
        let json = serde_json::to_string(&message).expect("message serializes");

        server.handle_text_frame(&json).expect("frame accepted");

        assert_eq!(
            server.snapshot_recent_events().unwrap(),
            vec!["accepted: stream config 1280x720@60".to_string()]
        );
    }

    #[test]
    fn spawned_server_reports_bound_address_and_stops() {
        let server = SignalingServer::new(HostConfig::new("hash"));
        let mut runtime = spawn_plain_ws_server(
            server,
            SignalingBindConfig {
                bind_addr: SocketAddr::from(([127, 0, 0, 1], 0)),
                security: TransportSecurity::PlainWsForVpn,
            },
        )
        .expect("server starts on ephemeral port");

        assert_ne!(runtime.bind_addr().port(), 0);
        runtime.stop();
    }
}
