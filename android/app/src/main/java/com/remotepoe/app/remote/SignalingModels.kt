package com.remotepoe.app.remote

import java.security.MessageDigest
import java.util.UUID

enum class SignalingType(val wireName: String) {
    Auth("auth"),
    DeviceInfo("device_info"),
    StreamConfig("stream_config"),
    Offer("offer"),
    Answer("answer"),
    Ice("ice"),
    InputEvent("input_event"),
    Error("error"),
}

data class SignalingMessage(
    val type: SignalingType,
    val requestId: String,
    val payload: SignalingPayload,
)

sealed interface SignalingPayload {
    data class Auth(
        val deviceId: String,
        val deviceName: String,
        val publicKey: String,
        val passwordHash: String,
    ) : SignalingPayload

    data class DeviceInfo(
        val deviceId: String,
        val deviceName: String,
        val appVersion: String,
        val supportsExternalKeyboard: Boolean,
        val supportsExternalMouse: Boolean,
    ) : SignalingPayload

    data class Stream(
        val width: Int,
        val height: Int,
        val fps: Int,
        val bitrateKbps: Int,
        val codec: String = "h264",
    ) : SignalingPayload

    data class Input(
        val event: RemoteInputEvent,
    ) : SignalingPayload
}

data class DeviceIdentity(
    val deviceId: String,
    val deviceName: String,
    val publicKey: String,
) {
    companion object {
        fun developmentDefault() = DeviceIdentity(
            deviceId = "android-dev-${UUID.randomUUID()}",
            deviceName = "Android development device",
            publicKey = "pending-device-public-key",
        )
    }
}

class SignalingSession(
    private val identity: DeviceIdentity,
    private val streamConfig: StreamConfig = StreamConfig.default1080p60(),
) {
    private val outbox = mutableListOf<SignalingMessage>()

    fun start(password: String): List<SignalingMessage> {
        outbox.clear()
        enqueue(
            SignalingType.Auth,
            SignalingPayload.Auth(
                deviceId = identity.deviceId,
                deviceName = identity.deviceName,
                publicKey = identity.publicKey,
                passwordHash = sha256(password),
            ),
        )
        enqueue(
            SignalingType.DeviceInfo,
            SignalingPayload.DeviceInfo(
                deviceId = identity.deviceId,
                deviceName = identity.deviceName,
                appVersion = "0.1.0",
                supportsExternalKeyboard = true,
                supportsExternalMouse = true,
            ),
        )
        enqueue(
            SignalingType.StreamConfig,
            SignalingPayload.Stream(
                width = streamConfig.width,
                height = streamConfig.height,
                fps = streamConfig.fps,
                bitrateKbps = streamConfig.bitrateKbps,
            ),
        )

        return snapshotOutbox()
    }

    fun sendInput(event: RemoteInputEvent) {
        if (event.isSingleUserAction()) {
            enqueue(SignalingType.InputEvent, SignalingPayload.Input(event))
        }
    }

    fun snapshotOutbox(): List<SignalingMessage> = outbox.toList()

    private fun enqueue(type: SignalingType, payload: SignalingPayload) {
        outbox += SignalingMessage(
            type = type,
            requestId = UUID.randomUUID().toString(),
            payload = payload,
        )
    }
}

private fun sha256(value: String): String {
    val digest = MessageDigest.getInstance("SHA-256").digest(value.toByteArray(Charsets.UTF_8))
    return digest.joinToString(separator = "") { byte -> "%02x".format(byte) }
}
