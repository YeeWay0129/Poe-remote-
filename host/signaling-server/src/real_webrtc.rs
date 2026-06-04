use crate::{WebRtcPeerError, WebRtcPeerGateway, WebRtcPeerResponse};
use host_core::signaling::IceCandidatePayload;
use media::Sample;
use media_pipeline::EncodedFrame;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::time::timeout;
use tokio::runtime::Runtime;
use webrtc::api::APIBuilder;
use webrtc::api::media_engine::MIME_TYPE_H264;
use webrtc::data_channel::RTCDataChannel;
use webrtc::ice_transport::ice_candidate::RTCIceCandidateInit;
use webrtc::peer_connection::RTCPeerConnection;
use webrtc::peer_connection::configuration::RTCConfiguration;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;
use webrtc::rtp_transceiver::rtp_codec::RTCRtpCodecCapability;
use webrtc::track::track_local::track_local_static_sample::TrackLocalStaticSample;

pub type ControlMessageHandler = Arc<dyn Fn(String) + Send + Sync>;

pub struct RealWebRtcPeerGateway {
    runtime: Runtime,
    peer_connection: Mutex<Option<Arc<RTCPeerConnection>>>,
    media_track: Mutex<Option<Arc<TrackLocalStaticSample>>>,
    control_message_handler: Option<ControlMessageHandler>,
}

impl RealWebRtcPeerGateway {
    pub fn new() -> Result<Self, WebRtcPeerError> {
        Self::new_with_control_handler(None)
    }

    pub fn new_with_control_handler(
        control_message_handler: Option<ControlMessageHandler>,
    ) -> Result<Self, WebRtcPeerError> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .map_err(|_| WebRtcPeerError::BackendUnavailable)?;

        Ok(Self {
            runtime,
            peer_connection: Mutex::new(None),
            media_track: Mutex::new(None),
            control_message_handler,
        })
    }

    async fn build_peer_connection(
        control_message_handler: Option<ControlMessageHandler>,
    ) -> Result<(Arc<RTCPeerConnection>, Arc<TrackLocalStaticSample>), WebRtcPeerError> {
        let api = APIBuilder::new().build();
        let peer_connection = Arc::new(
            api.new_peer_connection(RTCConfiguration::default())
                .await
                .map_err(|_| WebRtcPeerError::BackendUnavailable)?,
        );
        let video_track = Arc::new(TrackLocalStaticSample::new(
            RTCRtpCodecCapability {
                mime_type: MIME_TYPE_H264.to_owned(),
                ..Default::default()
            },
            "video".to_string(),
            "remote-poe".to_string(),
        ));
        peer_connection
            .add_track(video_track.clone())
            .await
            .map_err(|_| WebRtcPeerError::BackendUnavailable)?;

        peer_connection.on_data_channel(Box::new(move |data_channel: Arc<RTCDataChannel>| {
            let control_message_handler = control_message_handler.clone();
            Box::pin(async move {
                data_channel.on_message(Box::new(move |_message| {
                    let control_message_handler = control_message_handler.clone();
                    Box::pin(async move {
                        let Some(handler) = control_message_handler else {
                            return;
                        };
                        if let Ok(text) = String::from_utf8(_message.data.to_vec()) {
                            handler(text);
                        }
                    })
                }));
            })
        }));

        Ok((peer_connection, video_track))
    }

    async fn accept_offer_async(
        peer_connection: Arc<RTCPeerConnection>,
        offer_sdp: String,
    ) -> Result<WebRtcPeerResponse, WebRtcPeerError> {
        let offer =
            RTCSessionDescription::offer(offer_sdp).map_err(|_| WebRtcPeerError::InvalidOffer)?;
        peer_connection
            .set_remote_description(offer)
            .await
            .map_err(|_| WebRtcPeerError::InvalidOffer)?;

        let answer = peer_connection
            .create_answer(None)
            .await
            .map_err(|_| WebRtcPeerError::BackendUnavailable)?;
        let mut gathering_complete = peer_connection.gathering_complete_promise().await;
        peer_connection
            .set_local_description(answer)
            .await
            .map_err(|_| WebRtcPeerError::BackendUnavailable)?;
        let _ = timeout(Duration::from_secs(2), gathering_complete.recv()).await;
        let answer_sdp = peer_connection
            .local_description()
            .await
            .map(|description| description.sdp)
            .ok_or(WebRtcPeerError::BackendUnavailable)?;

        Ok(WebRtcPeerResponse {
            answer_sdp,
            ice_candidates: Vec::new(),
        })
    }
}

impl WebRtcPeerGateway for RealWebRtcPeerGateway {
    fn accept_offer(&self, offer_sdp: &str) -> Result<WebRtcPeerResponse, WebRtcPeerError> {
        if offer_sdp.trim().is_empty() {
            return Err(WebRtcPeerError::InvalidOffer);
        }

        let (peer_connection, media_track) = self.runtime.block_on(Self::build_peer_connection(
            self.control_message_handler.clone(),
        ))?;
        let response = self.runtime.block_on(Self::accept_offer_async(
            peer_connection.clone(),
            offer_sdp.to_string(),
        ))?;

        *self
            .peer_connection
            .lock()
            .map_err(|_| WebRtcPeerError::BackendUnavailable)? = Some(peer_connection);
        *self
            .media_track
            .lock()
            .map_err(|_| WebRtcPeerError::BackendUnavailable)? = Some(media_track);

        Ok(response)
    }

    fn add_remote_ice(&self, candidate: &IceCandidatePayload) -> Result<(), WebRtcPeerError> {
        if candidate.candidate.trim().is_empty() {
            return Err(WebRtcPeerError::InvalidOffer);
        }

        let peer_connection = self
            .peer_connection
            .lock()
            .map_err(|_| WebRtcPeerError::BackendUnavailable)?
            .clone()
            .ok_or(WebRtcPeerError::BackendUnavailable)?;
        let candidate = RTCIceCandidateInit {
            candidate: candidate.candidate.clone(),
            sdp_mid: candidate.sdp_mid.clone(),
            sdp_mline_index: candidate.sdp_m_line_index.map(|index| index as u16),
            username_fragment: None,
        };

        self.runtime
            .block_on(async move { peer_connection.add_ice_candidate(candidate).await })
            .map_err(|_| WebRtcPeerError::InvalidOffer)
    }

    fn push_encoded_frame(&self, frame: &EncodedFrame) -> Result<(), WebRtcPeerError> {
        let media_track = self
            .media_track
            .lock()
            .map_err(|_| WebRtcPeerError::BackendUnavailable)?
            .clone()
            .ok_or(WebRtcPeerError::BackendUnavailable)?;
        let sample = Sample {
            data: frame.data.clone().into(),
            duration: Duration::from_millis(16),
            ..Default::default()
        };

        self.runtime
            .block_on(async move { media_track.write_sample(&sample).await })
            .map_err(|_| WebRtcPeerError::BackendUnavailable)
    }
}
