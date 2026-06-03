use crate::{WebRtcPeerError, WebRtcPeerGateway, WebRtcPeerResponse};
use host_core::signaling::IceCandidatePayload;
use std::sync::{Arc, Mutex};
use tokio::runtime::Runtime;
use webrtc::api::APIBuilder;
use webrtc::data_channel::RTCDataChannel;
use webrtc::ice_transport::ice_candidate::RTCIceCandidateInit;
use webrtc::peer_connection::RTCPeerConnection;
use webrtc::peer_connection::configuration::RTCConfiguration;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;

pub struct RealWebRtcPeerGateway {
    runtime: Runtime,
    peer_connection: Mutex<Option<Arc<RTCPeerConnection>>>,
}

impl RealWebRtcPeerGateway {
    pub fn new() -> Result<Self, WebRtcPeerError> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .map_err(|_| WebRtcPeerError::BackendUnavailable)?;

        Ok(Self {
            runtime,
            peer_connection: Mutex::new(None),
        })
    }

    async fn build_peer_connection() -> Result<Arc<RTCPeerConnection>, WebRtcPeerError> {
        let api = APIBuilder::new().build();
        let peer_connection = Arc::new(
            api.new_peer_connection(RTCConfiguration::default())
                .await
                .map_err(|_| WebRtcPeerError::BackendUnavailable)?,
        );

        peer_connection.on_data_channel(Box::new(move |data_channel: Arc<RTCDataChannel>| {
            Box::pin(async move {
                data_channel.on_message(Box::new(move |_message| {
                    Box::pin(async move {
                        // Input injection is wired through the signaling server today.
                        // This callback reserves the control data channel path for the next stage.
                    })
                }));
            })
        }));

        Ok(peer_connection)
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
        let answer_sdp = answer.sdp.clone();
        peer_connection
            .set_local_description(answer)
            .await
            .map_err(|_| WebRtcPeerError::BackendUnavailable)?;

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

        let peer_connection = self.runtime.block_on(Self::build_peer_connection())?;
        let response = self.runtime.block_on(Self::accept_offer_async(
            peer_connection.clone(),
            offer_sdp.to_string(),
        ))?;

        *self
            .peer_connection
            .lock()
            .map_err(|_| WebRtcPeerError::BackendUnavailable)? = Some(peer_connection);

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
}
