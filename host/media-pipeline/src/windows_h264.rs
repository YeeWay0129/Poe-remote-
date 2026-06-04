use crate::{CapturedFrame, EncodeError, EncodedFrame, PixelFormat, VideoEncoder};
use host_core::stream::StreamConfig;
use std::mem::ManuallyDrop;
use std::ptr::null_mut;
use windows::core::Error;
use windows::Win32::Media::MediaFoundation::{
    IMFActivate, IMFMediaBuffer, IMFMediaType, IMFSample, IMFTransform, MFT_OUTPUT_DATA_BUFFER,
    MF_E_TRANSFORM_NEED_MORE_INPUT, MF_MT_AVG_BITRATE, MF_MT_FRAME_RATE, MF_MT_FRAME_SIZE,
    MF_MT_MAJOR_TYPE, MF_MT_SUBTYPE, MF_VERSION, MFCreateMediaType, MFCreateMemoryBuffer,
    MFCreateSample, MFMediaType_Video, MFSTARTUP_FULL, MFShutdown, MFStartup,
    MFT_CATEGORY_VIDEO_ENCODER, MFT_ENUM_FLAG_HARDWARE, MFT_ENUM_FLAG_SORTANDFILTER,
    MFT_MESSAGE_NOTIFY_BEGIN_STREAMING, MFT_MESSAGE_NOTIFY_START_OF_STREAM,
    MFT_REGISTER_TYPE_INFO, MFTEnumEx, MFVideoFormat_H264, MFVideoFormat_RGB32,
};
use windows::Win32::System::Com::CoTaskMemFree;

#[derive(Debug)]
pub struct WindowsMediaFoundationH264Encoder {
    discovered_encoders: u32,
    transform: Option<IMFTransform>,
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
        let transform = self
            .transform
            .as_ref()
            .ok_or(EncodeError::BackendUnavailable)?;
        encode_with_transform(transform, frame, config)
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

fn encode_with_transform(
    transform: &IMFTransform,
    frame: &CapturedFrame,
    config: &StreamConfig,
) -> Result<EncodedFrame, EncodeError> {
    if frame.pixel_format != PixelFormat::Bgra8
        || frame.width != config.width
        || frame.height != config.height
    {
        return Err(EncodeError::UnsupportedConfig);
    }

    let input_sample = create_input_sample(frame, config)?;
    unsafe {
        transform
            .ProcessInput(0, &input_sample, 0)
            .map_err(map_mf_encode_error)?;
    }

    let output_sample = create_output_sample(transform, frame)?;
    let mut output_data = MFT_OUTPUT_DATA_BUFFER {
        dwStreamID: 0,
        pSample: ManuallyDrop::new(Some(output_sample.clone())),
        dwStatus: 0,
        pEvents: ManuallyDrop::new(None),
    };
    let mut status = 0;
    let process_result =
        unsafe { transform.ProcessOutput(0, std::slice::from_mut(&mut output_data), &mut status) };
    cleanup_output_data_buffer(&mut output_data);

    match process_result {
        Ok(()) => {
            let data = read_sample_bytes(&output_sample)?;
            if data.is_empty() {
                return Err(EncodeError::BackendUnavailable);
            }

            Ok(EncodedFrame {
                codec: config.codec,
                timestamp_nanos: frame.timestamp_nanos,
                is_keyframe: false,
                data,
            })
        }
        Err(error) if error.code() == MF_E_TRANSFORM_NEED_MORE_INPUT => {
            Err(EncodeError::BackendUnavailable)
        }
        Err(error) => Err(map_mf_encode_error(error)),
    }
}

fn create_input_sample(
    frame: &CapturedFrame,
    config: &StreamConfig,
) -> Result<IMFSample, EncodeError> {
    let buffer = unsafe {
        MFCreateMemoryBuffer(frame.data.len() as u32).map_err(|_| EncodeError::BackendUnavailable)?
    };
    write_buffer_bytes(&buffer, &frame.data)?;

    let sample = unsafe { MFCreateSample().map_err(|_| EncodeError::BackendUnavailable)? };
    let sample_time = (frame.timestamp_nanos / 100) as i64;
    let sample_duration = 10_000_000_i64 / config.fps.max(1) as i64;

    unsafe {
        sample
            .AddBuffer(&buffer)
            .map_err(|_| EncodeError::BackendUnavailable)?;
        sample
            .SetSampleTime(sample_time)
            .map_err(|_| EncodeError::BackendUnavailable)?;
        sample
            .SetSampleDuration(sample_duration)
            .map_err(|_| EncodeError::BackendUnavailable)?;
    }

    Ok(sample)
}

fn create_output_sample(
    transform: &IMFTransform,
    frame: &CapturedFrame,
) -> Result<IMFSample, EncodeError> {
    let stream_info = unsafe {
        transform
            .GetOutputStreamInfo(0)
            .map_err(|_| EncodeError::BackendUnavailable)?
    };
    let encoded_buffer_size = stream_info.cbSize.max((frame.data.len() / 2).max(4096) as u32);
    let buffer = unsafe {
        MFCreateMemoryBuffer(encoded_buffer_size).map_err(|_| EncodeError::BackendUnavailable)?
    };
    let sample = unsafe { MFCreateSample().map_err(|_| EncodeError::BackendUnavailable)? };

    unsafe {
        sample
            .AddBuffer(&buffer)
            .map_err(|_| EncodeError::BackendUnavailable)?;
    }

    Ok(sample)
}

fn write_buffer_bytes(buffer: &IMFMediaBuffer, bytes: &[u8]) -> Result<(), EncodeError> {
    let mut data_ptr = null_mut();
    let mut max_len = 0;
    unsafe {
        buffer
            .Lock(&mut data_ptr, Some(&mut max_len), None)
            .map_err(|_| EncodeError::BackendUnavailable)?;
        if bytes.len() > max_len as usize {
            let _ = buffer.Unlock();
            return Err(EncodeError::UnsupportedConfig);
        }
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), data_ptr, bytes.len());
        let unlock_result = buffer.Unlock();
        unlock_result.map_err(|_| EncodeError::BackendUnavailable)?;
        buffer
            .SetCurrentLength(bytes.len() as u32)
            .map_err(|_| EncodeError::BackendUnavailable)?;
    }
    Ok(())
}

fn read_sample_bytes(sample: &IMFSample) -> Result<Vec<u8>, EncodeError> {
    let buffer = unsafe {
        sample
            .ConvertToContiguousBuffer()
            .map_err(|_| EncodeError::BackendUnavailable)?
    };
    let mut data_ptr = null_mut();
    let mut current_len = 0;
    let bytes = unsafe {
        buffer
            .Lock(&mut data_ptr, None, Some(&mut current_len))
            .map_err(|_| EncodeError::BackendUnavailable)?;
        let slice = std::slice::from_raw_parts(data_ptr, current_len as usize);
        let bytes = slice.to_vec();
        buffer.Unlock().map_err(|_| EncodeError::BackendUnavailable)?;
        bytes
    };
    Ok(bytes)
}

fn cleanup_output_data_buffer(output_data: &mut MFT_OUTPUT_DATA_BUFFER) {
    unsafe {
        let _ = ManuallyDrop::take(&mut output_data.pSample);
        let _ = ManuallyDrop::take(&mut output_data.pEvents);
    }
}

fn map_mf_encode_error(error: Error) -> EncodeError {
    if error.code() == MF_E_TRANSFORM_NEED_MORE_INPUT {
        EncodeError::BackendUnavailable
    } else {
        EncodeError::BackendUnavailable
    }
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
