use futures_util::{SinkExt, StreamExt};
use host_core::config::HostConfig;
use host_core::input::validate_user_event;
use host_core::pairing::{PairingDecision, PairingRequest, evaluate_pairing, is_trusted_device};
use host_core::signaling::{
    AuthPayload, ErrorPayload, SignalingMessage, SignalingPayload, SignalingType,
};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::oneshot;
use tokio_tungstenite::accept_async;
use tokio_tungstenite::tungstenite::Message;

pub type SharedHostConfig = Arc<Mutex<HostConfig>>;

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
    InputAccepted,
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
    UnsupportedMessage,
    StatePoisoned,
}

#[derive(Clone)]
pub struct SignalingServer {
    state: SharedHostConfig,
}

impl SignalingServer {
    pub fn new(config: HostConfig) -> Self {
        Self::from_shared_state(Arc::new(Mutex::new(config)))
    }

    pub fn from_shared_state(state: SharedHostConfig) -> Self {
        Self { state }
    }

    pub fn snapshot_config(&self) -> Result<HostConfig, SignalingFrameError> {
        self.state
            .lock()
            .map(|config| config.clone())
            .map_err(|_| SignalingFrameError::StatePoisoned)
    }

    pub fn handle_text_frame(&self, text: &str) -> Result<FrameResult, SignalingFrameError> {
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
                Ok(FrameResult::Accepted(ServerEvent::InputAccepted))
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
            Ok(FrameResult::Accepted(ServerEvent::InputAccepted))
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
