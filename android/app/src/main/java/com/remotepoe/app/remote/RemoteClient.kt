package com.remotepoe.app.remote

class RemoteClient {
    private var signalingSession = SignalingSession(DeviceIdentity.developmentDefault())

    fun connect(host: String, password: String): ConnectionState {
        if (host.isBlank() || password.isBlank()) {
            return ConnectionState.Disconnected
        }

        signalingSession.start(password)
        return ConnectionState.Connected
    }

    fun disconnect() {
        signalingSession = SignalingSession(DeviceIdentity.developmentDefault())
    }

    fun sendInput(event: RemoteInputEvent) {
        signalingSession.sendInput(event)
    }

    fun snapshotSignalingOutbox(): List<SignalingMessage> = signalingSession.snapshotOutbox()
}
