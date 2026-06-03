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

    fun connect(url: String, initialMessages: List<SignalingMessage>): TransportState

    fun send(message: SignalingMessage): Boolean

    fun close()
}

class RecordingSignalingTransport : SignalingTransport {
    private val sent = mutableListOf<String>()

    override var state: TransportState = TransportState.Disconnected
        private set

    override fun connect(url: String, initialMessages: List<SignalingMessage>): TransportState {
        if (url.isBlank()) {
            state = TransportState.Failed
            return state
        }

        state = TransportState.Connected
        initialMessages.forEach(::send)
        return state
    }

    override fun send(message: SignalingMessage): Boolean {
        if (state != TransportState.Connected) return false

        sent += message.toJsonString()
        return true
    }

    override fun close() {
        state = TransportState.Disconnected
        sent.clear()
    }

    fun snapshotSentJson(): List<String> = sent.toList()
}

class OkHttpSignalingTransport(
    private val client: OkHttpClient = OkHttpClient(),
) : SignalingTransport {
    private var socket: WebSocket? = null
    private val pendingMessages = mutableListOf<SignalingMessage>()

    override var state: TransportState = TransportState.Disconnected
        private set

    override fun connect(url: String, initialMessages: List<SignalingMessage>): TransportState {
        if (url.isBlank()) {
            state = TransportState.Failed
            return state
        }

        pendingMessages.clear()
        pendingMessages += initialMessages
        state = TransportState.Connecting
        socket = client.newWebSocket(
            Request.Builder().url(url).build(),
            object : WebSocketListener() {
                override fun onOpen(webSocket: WebSocket, response: Response) {
                    state = TransportState.Connected
                    pendingMessages.forEach { webSocket.send(it.toJsonString()) }
                    pendingMessages.clear()
                }

                override fun onFailure(webSocket: WebSocket, t: Throwable, response: Response?) {
                    state = TransportState.Failed
                    pendingMessages.clear()
                }

                override fun onClosed(webSocket: WebSocket, code: Int, reason: String) {
                    state = TransportState.Disconnected
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
        state = TransportState.Disconnected
    }
}
