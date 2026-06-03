use crate::{CapturedFrame, EncodeError, EncodedFrame, NullH264Encoder, VideoEncoder};
use host_core::stream::StreamConfig;
use std::ptr::null_mut;
use windows::Win32::Media::MediaFoundation::{
    IMFActivate, MF_VERSION, MFMediaType_Video, MFSTARTUP_FULL, MFShutdown, MFStartup,
    MFT_CATEGORY_VIDEO_ENCODER, MFT_ENUM_FLAG_HARDWARE, MFT_ENUM_FLAG_SORTANDFILTER,
    MFT_REGISTER_TYPE_INFO, MFTEnumEx, MFVideoFormat_H264,
};
use windows::Win32::System::Com::CoTaskMemFree;

#[derive(Debug)]
pub struct WindowsMediaFoundationH264Encoder {
    discovered_encoders: u32,
    fallback_encoder: NullH264Encoder,
}

impl WindowsMediaFoundationH264Encoder {
    pub fn new() -> Result<Self, EncodeError> {
        unsafe {
            MFStartup(MF_VERSION, MFSTARTUP_FULL).map_err(|_| EncodeError::BackendUnavailable)?;
        }

        let discovered_encoders = discover_hardware_h264_encoder_count()?;
        Ok(Self {
            discovered_encoders,
            fallback_encoder: NullH264Encoder::default(),
        })
    }

    pub fn discovered_encoders(&self) -> u32 {
        self.discovered_encoders
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

fn discover_hardware_h264_encoder_count() -> Result<u32, EncodeError> {
    let output_type = MFT_REGISTER_TYPE_INFO {
        guidMajorType: MFMediaType_Video,
        guidSubtype: MFVideoFormat_H264,
    };
    let mut activations: *mut Option<IMFActivate> = null_mut();
    let mut activation_count = 0;

    unsafe {
        MFTEnumEx(
            MFT_CATEGORY_VIDEO_ENCODER,
            MFT_ENUM_FLAG_HARDWARE | MFT_ENUM_FLAG_SORTANDFILTER,
            None,
            Some(&output_type),
            &mut activations,
            &mut activation_count,
        )
        .map_err(|_| EncodeError::BackendUnavailable)?;
    }

    release_activations(activations, activation_count);

    if activation_count == 0 {
        return Err(EncodeError::BackendUnavailable);
    }

    Ok(activation_count)
}

fn release_activations(activations: *mut Option<IMFActivate>, activation_count: u32) {
    if activations.is_null() {
        return;
    }

    unsafe {
        let activations = std::slice::from_raw_parts_mut(activations, activation_count as usize);
        for activation in activations.iter_mut() {
            let _ = activation.take();
        }
        CoTaskMemFree(Some(activations.as_ptr().cast()));
    }
}
