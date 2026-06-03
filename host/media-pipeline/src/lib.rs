use host_core::stream::{StreamConfig, VideoCodec};

#[cfg(all(feature = "windows-capture", windows))]
mod windows_capture;

#[cfg(all(feature = "windows-capture", windows))]
pub use windows_capture::WindowsGdiFrameSource;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PixelFormat {
    Bgra8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapturedFrame {
    pub width: u32,
    pub height: u32,
    pub pixel_format: PixelFormat,
    pub timestamp_nanos: u64,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncodedFrame {
    pub codec: VideoCodec,
    pub timestamp_nanos: u64,
    pub is_keyframe: bool,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureError {
    BackendUnavailable,
    UnsupportedConfig,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncodeError {
    BackendUnavailable,
    UnsupportedConfig,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipelineError {
    NotRunning,
    UnsupportedConfig,
    Capture(CaptureError),
    Encode(EncodeError),
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PipelineStats {
    pub captured_frames: u64,
    pub encoded_frames: u64,
    pub last_captured_bytes: Option<usize>,
    pub last_encoded_bytes: Option<usize>,
}

pub trait FrameSource: Send {
    fn capture_next(&mut self, config: &StreamConfig) -> Result<CapturedFrame, CaptureError>;
}

pub trait VideoEncoder: Send {
    fn encode(
        &mut self,
        frame: &CapturedFrame,
        config: &StreamConfig,
    ) -> Result<EncodedFrame, EncodeError>;
}

pub struct MediaPipeline {
    config: StreamConfig,
    source: Box<dyn FrameSource>,
    encoder: Box<dyn VideoEncoder>,
    running: bool,
    stats: PipelineStats,
}

impl MediaPipeline {
    pub fn new(
        config: StreamConfig,
        source: Box<dyn FrameSource>,
        encoder: Box<dyn VideoEncoder>,
    ) -> Result<Self, PipelineError> {
        if !config.is_supported_v1() {
            return Err(PipelineError::UnsupportedConfig);
        }

        Ok(Self {
            config,
            source,
            encoder,
            running: false,
            stats: PipelineStats::default(),
        })
    }

    pub fn recording(config: StreamConfig) -> Self {
        Self::new(
            config,
            Box::new(RecordingFrameSource::default()),
            Box::new(NullH264Encoder::default()),
        )
        .expect("default recording media pipeline config must be supported")
    }

    pub fn start(&mut self) {
        self.running = true;
    }

    pub fn stop(&mut self) {
        self.running = false;
    }

    pub fn is_running(&self) -> bool {
        self.running
    }

    pub fn config(&self) -> &StreamConfig {
        &self.config
    }

    pub fn set_config(&mut self, config: StreamConfig) -> Result<(), PipelineError> {
        if !config.is_supported_v1() {
            return Err(PipelineError::UnsupportedConfig);
        }

        self.config = config;
        Ok(())
    }

    pub fn stats(&self) -> PipelineStats {
        self.stats
    }

    pub fn capture_and_encode_once(&mut self) -> Result<EncodedFrame, PipelineError> {
        if !self.running {
            return Err(PipelineError::NotRunning);
        }

        let frame = self
            .source
            .capture_next(&self.config)
            .map_err(PipelineError::Capture)?;
        self.stats.captured_frames += 1;
        self.stats.last_captured_bytes = Some(frame.data.len());

        let encoded = self
            .encoder
            .encode(&frame, &self.config)
            .map_err(PipelineError::Encode)?;
        self.stats.encoded_frames += 1;
        self.stats.last_encoded_bytes = Some(encoded.data.len());

        Ok(encoded)
    }
}

#[derive(Debug, Default)]
pub struct RecordingFrameSource {
    next_frame_index: u64,
}

impl FrameSource for RecordingFrameSource {
    fn capture_next(&mut self, config: &StreamConfig) -> Result<CapturedFrame, CaptureError> {
        if !config.is_supported_v1() {
            return Err(CaptureError::UnsupportedConfig);
        }

        let frame_index = self.next_frame_index;
        self.next_frame_index += 1;

        Ok(CapturedFrame {
            width: config.width,
            height: config.height,
            pixel_format: PixelFormat::Bgra8,
            timestamp_nanos: frame_index.saturating_mul(16_666_667),
            data: Vec::new(),
        })
    }
}

#[derive(Debug, Default)]
pub struct NullH264Encoder {
    next_frame_index: u64,
}

impl VideoEncoder for NullH264Encoder {
    fn encode(
        &mut self,
        frame: &CapturedFrame,
        config: &StreamConfig,
    ) -> Result<EncodedFrame, EncodeError> {
        if config.codec != VideoCodec::H264 || !config.is_supported_v1() {
            return Err(EncodeError::UnsupportedConfig);
        }

        let frame_index = self.next_frame_index;
        self.next_frame_index += 1;

        Ok(EncodedFrame {
            codec: VideoCodec::H264,
            timestamp_nanos: frame.timestamp_nanos,
            is_keyframe: frame_index == 0,
            data: synthetic_h264_annex_b(frame.width, frame.height, frame_index),
        })
    }
}

fn synthetic_h264_annex_b(width: u32, height: u32, frame_index: u64) -> Vec<u8> {
    let mut data = vec![0, 0, 0, 1, 0x67];
    data.extend_from_slice(&width.to_be_bytes());
    data.extend_from_slice(&height.to_be_bytes());
    data.extend_from_slice(&frame_index.to_be_bytes());
    data
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pipeline_starts_and_stops() {
        let mut pipeline = MediaPipeline::recording(StreamConfig::default());

        assert!(!pipeline.is_running());
        pipeline.start();
        assert!(pipeline.is_running());
        pipeline.stop();
        assert!(!pipeline.is_running());
    }

    #[test]
    fn pipeline_rejects_unsupported_config() {
        let mut config = StreamConfig::default();
        config.fps = 120;

        let result = MediaPipeline::new(
            config,
            Box::new(RecordingFrameSource::default()),
            Box::new(NullH264Encoder::default()),
        );

        assert!(matches!(result, Err(PipelineError::UnsupportedConfig)));
    }

    #[test]
    fn capture_and_encode_requires_running_pipeline() {
        let mut pipeline = MediaPipeline::recording(StreamConfig::default());

        assert_eq!(
            pipeline.capture_and_encode_once(),
            Err(PipelineError::NotRunning)
        );
    }

    #[test]
    fn capture_and_encode_updates_stats() {
        let mut pipeline = MediaPipeline::recording(StreamConfig::fallback_720p60());
        pipeline.start();

        let frame = pipeline
            .capture_and_encode_once()
            .expect("recording pipeline encodes");

        assert_eq!(frame.codec, VideoCodec::H264);
        assert!(frame.is_keyframe);
        assert!(frame.data.starts_with(&[0, 0, 0, 1]));
        assert_eq!(
            pipeline.stats(),
            PipelineStats {
                captured_frames: 1,
                encoded_frames: 1,
                last_captured_bytes: Some(0),
                last_encoded_bytes: Some(frame.data.len()),
            }
        );
    }
}
