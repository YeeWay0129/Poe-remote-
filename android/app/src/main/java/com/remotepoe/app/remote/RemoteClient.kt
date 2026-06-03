package com.remotepoe.app.remote

class RemoteClient {
    private val sentEvents = mutableListOf<RemoteInputEvent>()

    fun connect(host: String, password: String): ConnectionState {
        if (host.isBlank() || password.isBlank()) {
            return ConnectionState.Disconnected
        }

        return ConnectionState.Connected
    }

    fun disconnect() {
        sentEvents.clear()
    }

    fun sendInput(event: RemoteInputEvent) {
        if (event.isSingleUserAction()) {
            sentEvents += event
        }
    }

    fun snapshotSentEvents(): List<RemoteInputEvent> = sentEvents.toList()
}
