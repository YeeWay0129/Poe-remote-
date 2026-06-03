package com.remotepoe.app.remote

import android.os.Handler
import android.os.Looper

class RemoteClient(
    private val transport: SignalingTransport = OkHttpSignalingTransport(),
) {
    private val mainHandler = Handler(Looper.getMainLooper())
    private var signalingSession = SignalingSession(DeviceIdentity.developmentDefault())

    var onConnectionChanged: ((ConnectionState, String?) -> Unit)? = null

    init {
        transport.onStateChanged = { transportState, message ->
            mainHandler.post {
                onConnectionChanged?.invoke(transportState.toConnectionState(), message)
            }
        }
    }

    fun connect(host: String, password: String): ConnectionState {
        if (host.isBlank() || password.isBlank()) {
            return ConnectionState.Failed
        }

        val url = host.toSignalingUrl()
        val initialMessages = signalingSession.start(password)
        return transport.connect(url, initialMessages).toConnectionState()
    }

    fun disconnect() {
        transport.close()
        signalingSession = SignalingSession(DeviceIdentity.developmentDefault())
    }

    fun sendInput(event: RemoteInputEvent) {
        signalingSession.sendInput(event)?.let(transport::send)
    }

    fun snapshotSignalingOutbox(): List<SignalingMessage> = signalingSession.snapshotOutbox()
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
