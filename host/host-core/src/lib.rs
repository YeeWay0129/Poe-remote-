pub mod config_store;

pub mod config {
    use crate::pairing::TrustedDevice;
    use crate::stream::StreamConfig;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct HostConfig {
        pub pairing_password_hash: String,
        pub trusted_devices: Vec<TrustedDevice>,
        pub stream: StreamConfig,
        pub autostart: bool,
    }

    impl HostConfig {
        pub fn new(pairing_password_hash: impl Into<String>) -> Self {
            Self {
                pairing_password_hash: pairing_password_hash.into(),
                trusted_devices: Vec::new(),
                stream: StreamConfig::default(),
                autostart: false,
            }
        }

        pub fn trust_device(&mut self, device: TrustedDevice) {
            if let Some(existing) = self
                .trusted_devices
                .iter_mut()
                .find(|candidate| candidate.device_id == device.device_id)
            {
                *existing = device;
                return;
            }

            self.trusted_devices.push(device);
        }

        pub fn revoke_device(&mut self, device_id: &str) -> bool {
            let before = self.trusted_devices.len();
            self.trusted_devices
                .retain(|device| device.device_id != device_id);
            before != self.trusted_devices.len()
        }
    }
}

pub mod pairing {
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct PairingRequest {
        pub device_id: String,
        pub device_name: String,
        pub public_key: String,
        pub password_hash: String,
    }

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct TrustedDevice {
        pub device_id: String,
        pub device_name: String,
        pub public_key: String,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum PairingDecision {
        Trusted(TrustedDevice),
        Rejected(PairingRejectReason),
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum PairingRejectReason {
        EmptyDeviceId,
        EmptyPublicKey,
        InvalidPassword,
    }

    pub fn evaluate_pairing(
        expected_password_hash: &str,
        request: PairingRequest,
    ) -> PairingDecision {
        if request.device_id.trim().is_empty() {
            return PairingDecision::Rejected(PairingRejectReason::EmptyDeviceId);
        }

        if request.public_key.trim().is_empty() {
            return PairingDecision::Rejected(PairingRejectReason::EmptyPublicKey);
        }

        if request.password_hash != expected_password_hash {
            return PairingDecision::Rejected(PairingRejectReason::InvalidPassword);
        }

        PairingDecision::Trusted(TrustedDevice {
            device_id: request.device_id,
            device_name: request.device_name,
            public_key: request.public_key,
        })
    }

    pub fn is_trusted_device(
        trusted_devices: &[TrustedDevice],
        device_id: &str,
        public_key: &str,
    ) -> bool {
        trusted_devices
            .iter()
            .any(|device| device.device_id == device_id && device.public_key == public_key)
    }
}

pub mod input {
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(tag = "kind", rename_all = "snake_case")]
    pub enum InputEvent {
        #[serde(rename_all = "camelCase")]
        Keyboard {
            key_code: u32,
            action: KeyAction,
        },
        #[serde(rename_all = "camelCase")]
        MouseMove {
            dx: i32,
            dy: i32,
            mode: PointerMode,
        },
        #[serde(rename_all = "camelCase")]
        MouseButton {
            button: MouseButton,
            action: ButtonAction,
        },
        #[serde(rename_all = "camelCase")]
        MouseWheel {
            delta_x: i32,
            delta_y: i32,
        },
        PointerModeChanged(PointerMode),
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum KeyAction {
        Down,
        Up,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum ButtonAction {
        Down,
        Up,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum MouseButton {
        Left,
        Right,
        Middle,
        Back,
        Forward,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum PointerMode {
        Relative,
        Absolute,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum InputRejectReason {
        KeyboardMacroLikeSequence,
        ZeroMouseMove,
        ZeroWheelDelta,
    }

    pub fn validate_user_event(event: &InputEvent) -> Result<(), InputRejectReason> {
        match event {
            InputEvent::Keyboard { .. } => Ok(()),
            InputEvent::MouseMove { dx, dy, .. } if *dx == 0 && *dy == 0 => {
                Err(InputRejectReason::ZeroMouseMove)
            }
            InputEvent::MouseMove { .. } => Ok(()),
            InputEvent::MouseButton { .. } => Ok(()),
            InputEvent::MouseWheel { delta_x, delta_y } if *delta_x == 0 && *delta_y == 0 => {
                Err(InputRejectReason::ZeroWheelDelta)
            }
            InputEvent::MouseWheel { .. } => Ok(()),
            InputEvent::PointerModeChanged(_) => Ok(()),
        }
    }
}

pub mod stream {
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct StreamConfig {
        pub width: u32,
        pub height: u32,
        pub fps: u32,
        pub bitrate_kbps: u32,
        pub codec: VideoCodec,
        pub display_id: Option<String>,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum VideoCodec {
        H264,
    }

    impl Default for StreamConfig {
        fn default() -> Self {
            Self {
                width: 1920,
                height: 1080,
                fps: 60,
                bitrate_kbps: 12_000,
                codec: VideoCodec::H264,
                display_id: None,
            }
        }
    }

    impl StreamConfig {
        pub fn fallback_720p60() -> Self {
            Self {
                width: 1280,
                height: 720,
                fps: 60,
                bitrate_kbps: 6_000,
                codec: VideoCodec::H264,
                display_id: None,
            }
        }

        pub fn is_supported_v1(&self) -> bool {
            self.codec == VideoCodec::H264
                && self.fps <= 60
                && matches!((self.width, self.height), (1920, 1080) | (1280, 720))
        }
    }
}

pub mod signaling {
    use crate::input::InputEvent;
    use crate::pairing::PairingRequest;
    use crate::stream::StreamConfig;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct SignalingMessage {
        #[serde(rename = "type")]
        pub message_type: SignalingType,
        pub request_id: String,
        pub payload: SignalingPayload,
    }

    impl SignalingMessage {
        pub fn auth(request_id: impl Into<String>, request: PairingRequest) -> Self {
            Self {
                message_type: SignalingType::Auth,
                request_id: request_id.into(),
                payload: SignalingPayload::Auth(AuthPayload {
                    device_id: request.device_id,
                    device_name: request.device_name,
                    public_key: request.public_key,
                    password_hash: request.password_hash,
                }),
            }
        }

        pub fn stream_config(request_id: impl Into<String>, config: StreamConfig) -> Self {
            Self {
                message_type: SignalingType::StreamConfig,
                request_id: request_id.into(),
                payload: SignalingPayload::StreamConfig(config),
            }
        }

        pub fn input_event(request_id: impl Into<String>, event: InputEvent) -> Self {
            Self {
                message_type: SignalingType::InputEvent,
                request_id: request_id.into(),
                payload: SignalingPayload::InputEvent(event),
            }
        }

        pub fn validate_shape(&self) -> Result<(), SignalingValidationError> {
            if self.request_id.trim().is_empty() {
                return Err(SignalingValidationError::EmptyRequestId);
            }

            let matches_type = matches!(
                (&self.message_type, &self.payload),
                (SignalingType::Auth, SignalingPayload::Auth(_))
                    | (SignalingType::DeviceInfo, SignalingPayload::DeviceInfo(_))
                    | (
                        SignalingType::StreamConfig,
                        SignalingPayload::StreamConfig(_)
                    )
                    | (
                        SignalingType::Offer,
                        SignalingPayload::SessionDescription(_)
                    )
                    | (
                        SignalingType::Answer,
                        SignalingPayload::SessionDescription(_)
                    )
                    | (SignalingType::Ice, SignalingPayload::Ice(_))
                    | (SignalingType::InputEvent, SignalingPayload::InputEvent(_))
                    | (SignalingType::Error, SignalingPayload::Error(_))
            );

            if matches_type {
                Ok(())
            } else {
                Err(SignalingValidationError::PayloadTypeMismatch)
            }
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum SignalingType {
        Auth,
        DeviceInfo,
        StreamConfig,
        Offer,
        Answer,
        Ice,
        InputEvent,
        Error,
    }

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum SignalingPayload {
        Auth(AuthPayload),
        DeviceInfo(DeviceInfoPayload),
        StreamConfig(StreamConfig),
        SessionDescription(SessionDescriptionPayload),
        Ice(IceCandidatePayload),
        InputEvent(InputEvent),
        Error(ErrorPayload),
    }

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct AuthPayload {
        pub device_id: String,
        pub device_name: String,
        pub public_key: String,
        pub password_hash: String,
    }

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct DeviceInfoPayload {
        pub device_id: String,
        pub device_name: String,
        pub app_version: String,
        pub supports_external_keyboard: bool,
        pub supports_external_mouse: bool,
    }

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct SessionDescriptionPayload {
        pub sdp: String,
    }

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct IceCandidatePayload {
        pub candidate: String,
        pub sdp_mid: Option<String>,
        pub sdp_m_line_index: Option<u32>,
    }

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct ErrorPayload {
        pub code: String,
        pub message: String,
        pub recoverable: bool,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum SignalingValidationError {
        EmptyRequestId,
        PayloadTypeMismatch,
    }
}

#[cfg(test)]
mod tests {
    use super::config::HostConfig;
    use super::input::{
        ButtonAction, InputEvent, InputRejectReason, KeyAction, MouseButton, PointerMode,
        validate_user_event,
    };
    use super::pairing::{
        PairingDecision, PairingRejectReason, PairingRequest, TrustedDevice, evaluate_pairing,
        is_trusted_device,
    };
    use super::signaling::{
        SignalingMessage, SignalingPayload, SignalingType, SignalingValidationError,
    };
    use super::stream::StreamConfig;

    #[test]
    fn pairing_trusts_device_when_password_hash_matches() {
        let request = PairingRequest {
            device_id: "tablet-1".to_string(),
            device_name: "Android tablet".to_string(),
            public_key: "public-key".to_string(),
            password_hash: "expected".to_string(),
        };

        let decision = evaluate_pairing("expected", request);

        assert_eq!(
            decision,
            PairingDecision::Trusted(TrustedDevice {
                device_id: "tablet-1".to_string(),
                device_name: "Android tablet".to_string(),
                public_key: "public-key".to_string(),
            })
        );
    }

    #[test]
    fn pairing_rejects_wrong_password_hash() {
        let request = PairingRequest {
            device_id: "phone-1".to_string(),
            device_name: "Android phone".to_string(),
            public_key: "public-key".to_string(),
            password_hash: "wrong".to_string(),
        };

        assert_eq!(
            evaluate_pairing("expected", request),
            PairingDecision::Rejected(PairingRejectReason::InvalidPassword)
        );
    }

    #[test]
    fn trusted_device_requires_id_and_public_key_match() {
        let trusted = vec![TrustedDevice {
            device_id: "phone-1".to_string(),
            device_name: "Android phone".to_string(),
            public_key: "public-key".to_string(),
        }];

        assert!(is_trusted_device(&trusted, "phone-1", "public-key"));
        assert!(!is_trusted_device(&trusted, "phone-1", "different-key"));
    }

    #[test]
    fn host_config_replaces_existing_trusted_device() {
        let mut config = HostConfig::new("hash");

        config.trust_device(TrustedDevice {
            device_id: "phone-1".to_string(),
            device_name: "Old".to_string(),
            public_key: "old-key".to_string(),
        });
        config.trust_device(TrustedDevice {
            device_id: "phone-1".to_string(),
            device_name: "New".to_string(),
            public_key: "new-key".to_string(),
        });

        assert_eq!(config.trusted_devices.len(), 1);
        assert_eq!(config.trusted_devices[0].public_key, "new-key");
    }

    #[test]
    fn stream_defaults_to_1080p60_h264() {
        let config = StreamConfig::default();

        assert_eq!(config.width, 1920);
        assert_eq!(config.height, 1080);
        assert_eq!(config.fps, 60);
        assert!(config.is_supported_v1());
    }

    #[test]
    fn stream_supports_720p60_fallback() {
        assert!(StreamConfig::fallback_720p60().is_supported_v1());
    }

    #[test]
    fn input_validation_rejects_noop_motion() {
        let event = InputEvent::MouseMove {
            dx: 0,
            dy: 0,
            mode: PointerMode::Relative,
        };

        assert_eq!(
            validate_user_event(&event),
            Err(InputRejectReason::ZeroMouseMove)
        );
    }

    #[test]
    fn input_validation_accepts_single_user_actions() {
        let events = [
            InputEvent::Keyboard {
                key_code: 87,
                action: KeyAction::Down,
            },
            InputEvent::MouseButton {
                button: MouseButton::Left,
                action: ButtonAction::Down,
            },
            InputEvent::MouseWheel {
                delta_x: 0,
                delta_y: -120,
            },
        ];

        for event in events {
            assert!(validate_user_event(&event).is_ok());
        }
    }

    #[test]
    fn signaling_serializes_auth_message_with_stable_shape() {
        let message = SignalingMessage::auth(
            "req-1",
            PairingRequest {
                device_id: "tablet-1".to_string(),
                device_name: "Tablet".to_string(),
                public_key: "public-key".to_string(),
                password_hash: "password-hash".to_string(),
            },
        );

        let json = serde_json::to_string(&message).expect("auth message serializes");

        assert!(json.contains(r#""type":"auth""#));
        assert!(json.contains(r#""requestId":"req-1""#));
        assert!(json.contains(r#""deviceId":"tablet-1""#));
    }

    #[test]
    fn signaling_round_trips_stream_config() {
        let message = SignalingMessage::stream_config("req-2", StreamConfig::fallback_720p60());
        let json = serde_json::to_string(&message).expect("stream config serializes");
        let parsed: SignalingMessage =
            serde_json::from_str(&json).expect("stream config deserializes");

        assert_eq!(parsed, message);
        assert_eq!(parsed.validate_shape(), Ok(()));
    }

    #[test]
    fn signaling_rejects_payload_type_mismatch() {
        let message = SignalingMessage {
            message_type: SignalingType::InputEvent,
            request_id: "req-3".to_string(),
            payload: SignalingPayload::StreamConfig(StreamConfig::default()),
        };

        assert_eq!(
            message.validate_shape(),
            Err(SignalingValidationError::PayloadTypeMismatch)
        );
    }
}
