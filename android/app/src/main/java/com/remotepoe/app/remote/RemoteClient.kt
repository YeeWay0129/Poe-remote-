package com.remotepoe.app.remote

import android.os.Handler
import android.os.Looper

class RemoteClient(
    private val transport: SignalingTransport = OkHttpSignalingTransport(),
    private val peerConnection: PeerConnectionGateway = NoopPeerConnectionGateway(),
) {
    private val mainHandler = Handler(Looper.getMainLooper())
    private var signalingSession = SignalingSession(DeviceIdentity.developmentDefault())
    private var offerStarted = false

    var onConnectionChanged: ((ConnectionState, String?) -> Unit)? = null

    init {
        transport.onStateChanged = { transportState, message ->
            mainHandler.post {
                if (transportState == TransportState.Connected) {
                    startPeerOffer()
                }
                onConnectionChanged?.invoke(transportState.toConnectionState(), message)
            }
        }
        transport.onMessageReceived = { message ->
            mainHandler.post { handleSignalingMessage(message) }
        }
        peerConnection.onLocalOffer = { sdp ->
            mainHandler.post { sendOffer(sdp) }
        }
        peerConnection.onLocalIceCandidate = { candidate ->
            mainHandler.post {
                sendIce(candidate.candidate, candidate.sdpMid, candidate.sdpMLineIndex)
            }
        }
        peerConnection.onError = { message ->
            mainHandler.post {
                onConnectionChanged?.invoke(ConnectionState.Failed, message)
            }
        }
    }

    fun connect(host: String, password: String): ConnectionState {
        if (host.isBlank() || password.isBlank()) {
            return ConnectionState.Failed
        }

        offerStarted = false
        val url = host.toSignalingUrl()
        val initialMessages = signalingSession.start(password)
        return transport.connect(url, initialMessages).toConnectionState()
    }

    fun disconnect() {
        transport.close()
        peerConnection.close()
        signalingSession = SignalingSession(DeviceIdentity.developmentDefault())
        offerStarted = false
    }

    fun sendInput(event: RemoteInputEvent) {
        signalingSession.sendInput(event)?.let(transport::send)
    }

    fun sendOffer(sdp: String) {
        signalingSession.sendOffer(sdp)?.let(transport::send)
    }

    fun sendAnswer(sdp: String) {
        signalingSession.sendAnswer(sdp)?.let(transport::send)
    }

    fun sendIce(candidate: String, sdpMid: String?, sdpMLineIndex: Int?) {
        signalingSession.sendIce(candidate, sdpMid, sdpMLineIndex)?.let(transport::send)
    }

    fun snapshotSignalingOutbox(): List<SignalingMessage> = signalingSession.snapshotOutbox()

    private fun startPeerOffer() {
        if (offerStarted) return

        offerStarted = true
        peerConnection.start()
        peerConnection.createOffer()
    }

    private fun handleSignalingMessage(message: SignalingMessage) {
        when (val payload = message.payload) {
            is SignalingPayload.SessionDescription -> {
                if (message.type == SignalingType.Answer) {
                    peerConnection.setRemoteAnswer(payload.sdp)
                }
            }

            is SignalingPayload.Ice -> {
                peerConnection.addRemoteIceCandidate(
                    RemoteIceCandidate(
                        candidate = payload.candidate,
                        sdpMid = payload.sdpMid,
                        sdpMLineIndex = payload.sdpMLineIndex,
                    ),
                )
            }

            is SignalingPayload.Error -> {
                onConnectionChanged?.invoke(ConnectionState.Failed, payload.message)
            }

            else -> Unit
        }
    }
}

private fun String.toSignalingUrl(): String =
    when {
        startsWith("ws://") || startsWith("wss://") -> this
        else -> "ws://$this/signaling"
    }

private fun TransportState.toConnectionState(): ConnectionState =
    when (this) {
        TransportState.Disconnected -> ConnectionState.Disconnected
        TransportState.Connecting -> ConnectionState.Connecting
        TransportState.Connected -> ConnectionState.Connected
        TransportState.Failed -> ConnectionState.Failed
    }
