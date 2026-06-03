package com.remotepoe.app.remote

class RemoteClient(
    private val transport: SignalingTransport = OkHttpSignalingTransport(),
) {
    private var signalingSession = SignalingSession(DeviceIdentity.developmentDefault())

    fun connect(host: String, password: String): ConnectionState {
        if (host.isBlank() || password.isBlank()) {
            return ConnectionState.Disconnected
        }

        val url = host.toSignalingUrl()
        val initialMessages = signalingSession.start(password)
        return when (transport.connect(url, initialMessages)) {
            TransportState.Connected,
            TransportState.Connecting -> ConnectionState.Connected

            TransportState.Disconnected,
            TransportState.Failed -> ConnectionState.Disconnected
        }
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
