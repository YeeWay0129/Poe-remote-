use crate::{CapturedFrame, EncodeError, EncodedFrame, NullH264Encoder, VideoEncoder};
use host_core::stream::StreamConfig;
use windows::Win32::Media::MediaFoundation::{MF_VERSION, MFSTARTUP_FULL, MFShutdown, MFStartup};

#[derive(Debug)]
pub struct WindowsMediaFoundationH264Encoder {
    fallback_encoder: NullH264Encoder,
}

impl WindowsMediaFoundationH264Encoder {
    pub fn new() -> Result<Self, EncodeError> {
        unsafe {
            MFStartup(MF_VERSION, MFSTARTUP_FULL).map_err(|_| EncodeError::BackendUnavailable)?;
        }

        Ok(Self {
            fallback_encoder: NullH264Encoder::default(),
        })
    }
}

impl VideoEncoder for WindowsMediaFoundationH264Encoder {
    fn encode(
        &mut self,
        frame: &CapturedFrame,
        config: &StreamConfig,
    ) -> Result<EncodedFrame, EncodeError> {
        // Media Foundation startup is verified here; MFT hardware processing is the next backend step.
        self.fallback_encoder.encode(frame, config)
    }
}

impl Drop for WindowsMediaFoundationH264Encoder {
    fn drop(&mut self) {
        unsafe {
            let _ = MFShutdown();
        }
    }
}
