use crate::{CapturedFrame, EncodeError, EncodedFrame, NullH264Encoder, VideoEncoder};
use host_core::stream::StreamConfig;
use std::ptr::null_mut;
use windows::Win32::Media::MediaFoundation::{
    IMFActivate, IMFMediaType, IMFTransform, MF_MT_AVG_BITRATE, MF_MT_FRAME_RATE, MF_MT_FRAME_SIZE,
    MF_MT_MAJOR_TYPE, MF_MT_SUBTYPE, MF_VERSION, MFCreateMediaType, MFMediaType_Video,
    MFSTARTUP_FULL, MFShutdown, MFStartup, MFT_CATEGORY_VIDEO_ENCODER, MFT_ENUM_FLAG_HARDWARE,
    MFT_ENUM_FLAG_SORTANDFILTER, MFT_MESSAGE_NOTIFY_BEGIN_STREAMING,
    MFT_MESSAGE_NOTIFY_START_OF_STREAM, MFT_REGISTER_TYPE_INFO, MFTEnumEx, MFVideoFormat_H264,
    MFVideoFormat_RGB32,
};
use windows::Win32::System::Com::CoTaskMemFree;

#[derive(Debug)]
pub struct WindowsMediaFoundationH264Encoder {
    discovered_encoders: u32,
    transform: Option<IMFTransform>,
    fallback_encoder: NullH264Encoder,
}

// The MFT is created lazily on the media thread and then used by that same pipeline loop.
unsafe impl Send for WindowsMediaFoundationH264Encoder {}

impl WindowsMediaFoundationH264Encoder {
    pub fn new() -> Result<Self, EncodeError> {
        unsafe {
            MFStartup(MF_VERSION, MFSTARTUP_FULL).map_err(|_| EncodeError::BackendUnavailable)?;
        }

        let discovered_encoders = discover_hardware_h264_encoder_count()?;
        Ok(Self {
            discovered_encoders,
            transform: None,
            fallback_encoder: NullH264Encoder::default(),
        })
    }

    pub fn discovered_encoders(&self) -> u32 {
        self.discovered_encoders
    }

    fn ensure_transform(&mut self, config: &StreamConfig) -> Result<(), EncodeError> {
        if self.transform.is_some() {
            return Ok(());
        }

        let transform = activate_hardware_h264_encoder()?;
        configure_h264_transform(&transform, config)?;
        self.transform = Some(transform);
        Ok(())
    }
}

impl VideoEncoder for WindowsMediaFoundationH264Encoder {
    fn encode(
        &mut self,
        frame: &CapturedFrame,
        config: &StreamConfig,
    ) -> Result<EncodedFrame, EncodeError> {
        self.ensure_transform(config)?;
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
    let (_activations, activation_count) = enum_hardware_h264_encoder_activations()?;
    Ok(activation_count)
}

fn activate_hardware_h264_encoder() -> Result<IMFTransform, EncodeError> {
    let (mut activations, activation_count) = enum_hardware_h264_encoder_activations()?;
    if activation_count == 0 || activations.is_empty() {
        return Err(EncodeError::BackendUnavailable);
    }

    let first = activations[0]
        .take()
        .ok_or(EncodeError::BackendUnavailable)?;
    let transform = unsafe {
        first
            .ActivateObject::<IMFTransform>()
            .map_err(|_| EncodeError::BackendUnavailable)?
    };

    Ok(transform)
}

fn enum_hardware_h264_encoder_activations() -> Result<(ActivationList, u32), EncodeError> {
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

    if activation_count == 0 {
        return Err(EncodeError::BackendUnavailable);
    }

    Ok((
        ActivationList::new(activations, activation_count),
        activation_count,
    ))
}

struct ActivationList {
    activations: *mut Option<IMFActivate>,
    activation_count: u32,
}

impl ActivationList {
    fn new(activations: *mut Option<IMFActivate>, activation_count: u32) -> Self {
        Self {
            activations,
            activation_count,
        }
    }

    fn is_empty(&self) -> bool {
        self.activation_count == 0 || self.activations.is_null()
    }
}

impl std::ops::Index<usize> for ActivationList {
    type Output = Option<IMFActivate>;

    fn index(&self, index: usize) -> &Self::Output {
        unsafe { &*self.activations.add(index) }
    }
}

impl std::ops::IndexMut<usize> for ActivationList {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        unsafe { &mut *self.activations.add(index) }
    }
}

impl Drop for ActivationList {
    fn drop(&mut self) {
        if self.activations.is_null() {
            return;
        }

        unsafe {
            let activations =
                std::slice::from_raw_parts_mut(self.activations, self.activation_count as usize);
            for activation in activations.iter_mut() {
                let _ = activation.take();
            }
            CoTaskMemFree(Some(self.activations.cast()));
        }
    }
}

fn configure_h264_transform(
    transform: &IMFTransform,
    config: &StreamConfig,
) -> Result<(), EncodeError> {
    let output_type = create_video_media_type(
        config,
        &MFVideoFormat_H264,
        Some(config.bitrate_kbps.saturating_mul(1000)),
    )?;
    let input_type = create_video_media_type(config, &MFVideoFormat_RGB32, None)?;

    unsafe {
        transform
            .SetOutputType(0, &output_type, 0)
            .map_err(|_| EncodeError::UnsupportedConfig)?;
        transform
            .SetInputType(0, &input_type, 0)
            .map_err(|_| EncodeError::UnsupportedConfig)?;
        transform
            .ProcessMessage(MFT_MESSAGE_NOTIFY_BEGIN_STREAMING, 0)
            .map_err(|_| EncodeError::BackendUnavailable)?;
        transform
            .ProcessMessage(MFT_MESSAGE_NOTIFY_START_OF_STREAM, 0)
            .map_err(|_| EncodeError::BackendUnavailable)?;
    }

    Ok(())
}

fn create_video_media_type(
    config: &StreamConfig,
    subtype: &windows::core::GUID,
    bitrate: Option<u32>,
) -> Result<IMFMediaType, EncodeError> {
    let media_type = unsafe { MFCreateMediaType().map_err(|_| EncodeError::BackendUnavailable)? };
    let frame_size = ((config.width as u64) << 32) | config.height as u64;
    let frame_rate = ((config.fps as u64) << 32) | 1;

    unsafe {
        media_type
            .SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)
            .map_err(|_| EncodeError::BackendUnavailable)?;
        media_type
            .SetGUID(&MF_MT_SUBTYPE, subtype)
            .map_err(|_| EncodeError::BackendUnavailable)?;
        media_type
            .SetUINT64(&MF_MT_FRAME_SIZE, frame_size)
            .map_err(|_| EncodeError::BackendUnavailable)?;
        media_type
            .SetUINT64(&MF_MT_FRAME_RATE, frame_rate)
            .map_err(|_| EncodeError::BackendUnavailable)?;
        if let Some(bitrate) = bitrate {
            media_type
                .SetUINT32(&MF_MT_AVG_BITRATE, bitrate)
                .map_err(|_| EncodeError::BackendUnavailable)?;
        }
    }

    Ok(media_type)
}
