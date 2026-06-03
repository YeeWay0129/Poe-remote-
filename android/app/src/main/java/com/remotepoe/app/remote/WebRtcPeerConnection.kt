package com.remotepoe.app.remote

import android.content.Context
import java.nio.ByteBuffer
import java.nio.charset.StandardCharsets
import org.webrtc.DataChannel
import org.webrtc.IceCandidate
import org.webrtc.MediaConstraints
import org.webrtc.PeerConnection
import org.webrtc.PeerConnectionFactory
import org.webrtc.SdpObserver
import org.webrtc.SessionDescription
import org.webrtc.VideoTrack

data class RemoteIceCandidate(
    val candidate: String,
    val sdpMid: String?,
    val sdpMLineIndex: Int?,
)

interface PeerConnectionGateway {
    var onLocalOffer: ((String) -> Unit)?
    var onLocalIceCandidate: ((RemoteIceCandidate) -> Unit)?
    var onRemoteVideoTrack: ((VideoTrack) -> Unit)?
    var onError: ((String) -> Unit)?

    fun start()
    fun createOffer()
    fun setRemoteAnswer(sdp: String)
    fun addRemoteIceCandidate(candidate: RemoteIceCandidate)
    fun sendControlMessage(json: String): Boolean
    fun close()
}

class NoopPeerConnectionGateway : PeerConnectionGateway {
    override var onLocalOffer: ((String) -> Unit)? = null
    override var onLocalIceCandidate: ((RemoteIceCandidate) -> Unit)? = null
    override var onRemoteVideoTrack: ((VideoTrack) -> Unit)? = null
    override var onError: ((String) -> Unit)? = null

    override fun start() = Unit
    override fun createOffer() = Unit
    override fun setRemoteAnswer(sdp: String) = Unit
    override fun addRemoteIceCandidate(candidate: RemoteIceCandidate) = Unit
    override fun sendControlMessage(json: String): Boolean = false
    override fun close() = Unit
}

class AndroidWebRtcPeerConnectionGateway(
    context: Context,
) : PeerConnectionGateway {
    private val appContext = context.applicationContext
    private var factory: PeerConnectionFactory? = null
    private var peerConnection: PeerConnection? = null
    private var controlChannel: DataChannel? = null
    private var pendingLocalOffer: SessionDescription? = null

    override var onLocalOffer: ((String) -> Unit)? = null
    override var onLocalIceCandidate: ((RemoteIceCandidate) -> Unit)? = null
    override var onRemoteVideoTrack: ((VideoTrack) -> Unit)? = null
    override var onError: ((String) -> Unit)? = null

    override fun start() {
        ensurePeerConnection()
    }

    override fun createOffer() {
        val connection = ensurePeerConnection()
        val constraints = MediaConstraints().apply {
            mandatory.add(MediaConstraints.KeyValuePair("OfferToReceiveAudio", "false"))
            mandatory.add(MediaConstraints.KeyValuePair("OfferToReceiveVideo", "true"))
        }

        connection.createOffer(
            object : SdpObserverAdapter() {
                override fun onCreateSuccess(description: SessionDescription) {
                    pendingLocalOffer = description
                    connection.setLocalDescription(this, description)
                }

                override fun onSetSuccess() {
                    pendingLocalOffer?.let { offer ->
                        onLocalOffer?.invoke(offer.description)
                        pendingLocalOffer = null
                    }
                }

                override fun onCreateFailure(error: String) {
                    onError?.invoke("Failed to create WebRTC offer: $error")
                }

                override fun onSetFailure(error: String) {
                    onError?.invoke("Failed to set local WebRTC offer: $error")
                    pendingLocalOffer = null
                }
            },
            constraints,
        )
    }

    override fun setRemoteAnswer(sdp: String) {
        if (sdp.isBlank()) return

        ensurePeerConnection().setRemoteDescription(
            object : SdpObserverAdapter() {
                override fun onSetFailure(error: String) {
                    onError?.invoke("Failed to set remote WebRTC answer: $error")
                }
            },
            SessionDescription(SessionDescription.Type.ANSWER, sdp),
        )
    }

    override fun addRemoteIceCandidate(candidate: RemoteIceCandidate) {
        if (candidate.candidate.isBlank()) return

        ensurePeerConnection().addIceCandidate(
            IceCandidate(
                candidate.sdpMid,
                candidate.sdpMLineIndex ?: 0,
                candidate.candidate,
            ),
        )
    }

    override fun sendControlMessage(json: String): Boolean {
        if (json.isBlank()) return false

        val channel = controlChannel ?: return false
        if (channel.state() != DataChannel.State.OPEN) return false

        val bytes = json.toByteArray(StandardCharsets.UTF_8)
        return channel.send(DataChannel.Buffer(ByteBuffer.wrap(bytes), false))
    }

    override fun close() {
        controlChannel?.dispose()
        controlChannel = null
        peerConnection?.dispose()
        peerConnection = null
        factory?.dispose()
        factory = null
        pendingLocalOffer = null
    }

    private fun ensurePeerConnection(): PeerConnection {
        peerConnection?.let { return it }

        val nextFactory = factory ?: createFactory().also { factory = it }
        val rtcConfig = PeerConnection.RTCConfiguration(emptyList()).apply {
            sdpSemantics = PeerConnection.SdpSemantics.UNIFIED_PLAN
            continualGatheringPolicy = PeerConnection.ContinualGatheringPolicy.GATHER_CONTINUALLY
        }
        val connection = nextFactory.createPeerConnection(rtcConfig, peerObserver())
            ?: error("Failed to create WebRTC peer connection.")
        controlChannel = connection.createDataChannel("control", DataChannel.Init())
        peerConnection = connection
        return connection
    }

    private fun createFactory(): PeerConnectionFactory {
        PeerConnectionFactory.initialize(
            PeerConnectionFactory.InitializationOptions.builder(appContext)
                .setEnableInternalTracer(false)
                .createInitializationOptions(),
        )

        return PeerConnectionFactory.builder()
            .setOptions(PeerConnectionFactory.Options())
            .createPeerConnectionFactory()
    }

    private fun peerObserver(): PeerConnection.Observer =
        object : PeerConnection.Observer {
            override fun onIceCandidate(candidate: IceCandidate) {
                onLocalIceCandidate?.invoke(
                    RemoteIceCandidate(
                        candidate = candidate.sdp,
                        sdpMid = candidate.sdpMid,
                        sdpMLineIndex = candidate.sdpMLineIndex,
                    ),
                )
            }

            override fun onSignalingChange(state: PeerConnection.SignalingState) = Unit
            override fun onIceConnectionChange(state: PeerConnection.IceConnectionState) = Unit
            override fun onIceConnectionReceivingChange(receiving: Boolean) = Unit
            override fun onIceGatheringChange(state: PeerConnection.IceGatheringState) = Unit
            override fun onIceCandidatesRemoved(candidates: Array<out IceCandidate>) = Unit
            override fun onAddStream(stream: org.webrtc.MediaStream) = Unit
            override fun onRemoveStream(stream: org.webrtc.MediaStream) = Unit
            override fun onDataChannel(channel: DataChannel) = Unit
            override fun onRenegotiationNeeded() = Unit
            override fun onAddTrack(
                receiver: org.webrtc.RtpReceiver,
                streams: Array<out org.webrtc.MediaStream>,
            ) {
                val track = receiver.track()
                if (track is VideoTrack) {
                    track.setEnabled(true)
                    onRemoteVideoTrack?.invoke(track)
                }
            }
        }
}

open class SdpObserverAdapter : SdpObserver {
    override fun onCreateSuccess(description: SessionDescription) = Unit
    override fun onSetSuccess() = Unit
    override fun onCreateFailure(error: String) = Unit
    override fun onSetFailure(error: String) = Unit
}
