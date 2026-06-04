package com.remotepoe.app.remote

import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener

enum class TransportState {
    Disconnected,
    Connecting,
    Connected,
    Failed,
}

interface SignalingTransport {
    val state: TransportState
    var onStateChanged: ((TransportState, String?) -> Unit)?
    var onMessageReceived: ((SignalingMessage) -> Unit)?

    fun connect(url: String, initialMessages: List<SignalingMessage>): TransportState

    fun send(message: SignalingMessage): Boolean

    fun close()
}

class RecordingSignalingTransport : SignalingTransport {
    private val sent = mutableListOf<String>()

    override var state: TransportState = TransportState.Disconnected
        private set

    override var onStateChanged: ((TransportState, String?) -> Unit)? = null
    override var onMessageReceived: ((SignalingMessage) -> Unit)? = null

    override fun connect(url: String, initialMessages: List<SignalingMessage>): TransportState {
        if (url.isBlank()) {
            updateState(TransportState.Failed, "Signaling URL is blank.")
            return state
        }

        updateState(TransportState.Connected, null)
        initialMessages.forEach(::send)
        return state
    }

    override fun send(message: SignalingMessage): Boolean {
        if (state != TransportState.Connected) return false

        sent += message.toJsonString()
        return true
    }

    override fun close() {
        updateState(TransportState.Disconnected, null)
        sent.clear()
    }

    fun snapshotSentJson(): List<String> = sent.toList()

    fun receive(json: String) {
        parseSignalingMessage(json)?.let { onMessageReceived?.invoke(it) }
    }

    private fun updateState(nextState: TransportState, message: String?) {
        state = nextState
        onStateChanged?.invoke(nextState, message)
    }
}

class OkHttpSignalingTransport(
    private val client: OkHttpClient = OkHttpClient(),
) : SignalingTransport {
    private var socket: WebSocket? = null
    private val pendingMessages = mutableListOf<SignalingMessage>()

    override var state: TransportState = TransportState.Disconnected
        private set

    override var onStateChanged: ((TransportState, String?) -> Unit)? = null
    override var onMessageReceived: ((SignalingMessage) -> Unit)? = null

    override fun connect(url: String, initialMessages: List<SignalingMessage>): TransportState {
        if (url.isBlank()) {
            updateState(TransportState.Failed, "Signaling URL is blank.")
            return state
        }

        pendingMessages.clear()
        pendingMessages += initialMessages
        updateState(TransportState.Connecting, null)
        val request = try {
            Request.Builder().url(url).build()
        } catch (error: IllegalArgumentException) {
            pendingMessages.clear()
            updateState(TransportState.Failed, error.message ?: "Invalid signaling URL.")
            return state
        }
        socket = client.newWebSocket(
            request,
            object : WebSocketListener() {
                override fun onOpen(webSocket: WebSocket, response: Response) {
                    updateState(TransportState.Connected, null)
                    pendingMessages.forEach { webSocket.send(it.toJsonString()) }
                    pendingMessages.clear()
                }

                override fun onFailure(webSocket: WebSocket, t: Throwable, response: Response?) {
                    updateState(TransportState.Failed, t.message ?: "WebSocket connection failed.")
                    pendingMessages.clear()
                }

                override fun onMessage(webSocket: WebSocket, text: String) {
                    parseSignalingMessage(text)?.let { onMessageReceived?.invoke(it) }
                }

                override fun onClosed(webSocket: WebSocket, code: Int, reason: String) {
                    updateState(TransportState.Disconnected, reason.ifBlank { null })
                    pendingMessages.clear()
                }
            },
        )

        return state
    }

    override fun send(message: SignalingMessage): Boolean {
        val activeSocket = socket ?: return false
        if (state != TransportState.Connected) return false

        return activeSocket.send(message.toJsonString())
    }

    override fun close() {
        socket?.close(1000, "client disconnect")
        socket = null
        pendingMessages.clear()
        updateState(TransportState.Disconnected, null)
    }

    private fun updateState(nextState: TransportState, message: String?) {
        state = nextState
        onStateChanged?.invoke(nextState, message)
    }
}
