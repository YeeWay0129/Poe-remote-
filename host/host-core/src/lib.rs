pub mod config {
    use crate::pairing::TrustedDevice;
    use crate::stream::StreamConfig;

    #[derive(Debug, Clone, PartialEq, Eq)]
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
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct PairingRequest {
        pub device_id: String,
        pub device_name: String,
        pub public_key: String,
        pub password_hash: String,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
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
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum InputEvent {
        Keyboard {
            key_code: u32,
            action: KeyAction,
        },
        MouseMove {
            dx: i32,
            dy: i32,
            mode: PointerMode,
        },
        MouseButton {
            button: MouseButton,
            action: ButtonAction,
        },
        MouseWheel {
            delta_x: i32,
            delta_y: i32,
        },
        PointerModeChanged(PointerMode),
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum KeyAction {
        Down,
        Up,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum ButtonAction {
        Down,
        Up,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum MouseButton {
        Left,
        Right,
        Middle,
        Back,
        Forward,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct StreamConfig {
        pub width: u32,
        pub height: u32,
        pub fps: u32,
        pub bitrate_kbps: u32,
        pub codec: VideoCodec,
        pub display_id: Option<String>,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
}
