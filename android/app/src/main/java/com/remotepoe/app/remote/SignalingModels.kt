package com.remotepoe.app.remote

import java.security.MessageDigest
import java.util.UUID
import org.json.JSONObject

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

fun parseSignalingMessage(json: String): SignalingMessage? =
    runCatching {
        val root = JSONObject(json)
        val type = root.getString("type").toSignalingType()
        val requestId = root.getString("requestId")
        val payload = root.getJSONObject("payload").toSignalingPayload(type)
        SignalingMessage(type, requestId, payload)
    }.getOrNull()

fun SignalingMessage.toJsonString(): String =
    JSONObject()
        .put("type", type.wireName)
        .put("requestId", requestId)
        .put("payload", payload.toJsonObject())
        .toString()

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

    data class SessionDescription(
        val sdp: String,
    ) : SignalingPayload

    data class Ice(
        val candidate: String,
        val sdpMid: String?,
        val sdpMLineIndex: Int?,
    ) : SignalingPayload

    data class Input(
        val event: RemoteInputEvent,
    ) : SignalingPayload

    data class Error(
        val code: String,
        val message: String,
        val recoverable: Boolean,
    ) : SignalingPayload
}

private fun SignalingPayload.toJsonObject(): JSONObject =
    when (this) {
        is SignalingPayload.Auth -> JSONObject()
            .put("deviceId", deviceId)
            .put("deviceName", deviceName)
            .put("publicKey", publicKey)
            .put("passwordHash", passwordHash)

        is SignalingPayload.DeviceInfo -> JSONObject()
            .put("deviceId", deviceId)
            .put("deviceName", deviceName)
            .put("appVersion", appVersion)
            .put("supportsExternalKeyboard", supportsExternalKeyboard)
            .put("supportsExternalMouse", supportsExternalMouse)

        is SignalingPayload.Stream -> JSONObject()
            .put("width", width)
            .put("height", height)
            .put("fps", fps)
            .put("bitrateKbps", bitrateKbps)
            .put("codec", codec)
            .put("displayId", JSONObject.NULL)

        is SignalingPayload.SessionDescription -> JSONObject()
            .put("sdp", sdp)

        is SignalingPayload.Ice -> JSONObject()
            .put("candidate", candidate)
            .put("sdpMid", sdpMid ?: JSONObject.NULL)
            .put("sdpMLineIndex", sdpMLineIndex ?: JSONObject.NULL)

        is SignalingPayload.Input -> event.toJsonObject()

        is SignalingPayload.Error -> JSONObject()
            .put("code", code)
            .put("message", message)
            .put("recoverable", recoverable)
    }

private fun String.toSignalingType(): SignalingType =
    SignalingType.entries.first { type -> type.wireName == this }

private fun JSONObject.toSignalingPayload(type: SignalingType): SignalingPayload =
    when (type) {
        SignalingType.Auth -> SignalingPayload.Auth(
            deviceId = getString("deviceId"),
            deviceName = getString("deviceName"),
            publicKey = getString("publicKey"),
            passwordHash = getString("passwordHash"),
        )

        SignalingType.DeviceInfo -> SignalingPayload.DeviceInfo(
            deviceId = getString("deviceId"),
            deviceName = getString("deviceName"),
            appVersion = getString("appVersion"),
            supportsExternalKeyboard = getBoolean("supportsExternalKeyboard"),
            supportsExternalMouse = getBoolean("supportsExternalMouse"),
        )

        SignalingType.StreamConfig -> SignalingPayload.Stream(
            width = getInt("width"),
            height = getInt("height"),
            fps = getInt("fps"),
            bitrateKbps = getInt("bitrateKbps"),
            codec = optString("codec", "h264"),
        )

        SignalingType.Offer,
        SignalingType.Answer -> SignalingPayload.SessionDescription(
            sdp = getString("sdp"),
        )

        SignalingType.Ice -> SignalingPayload.Ice(
            candidate = getString("candidate"),
            sdpMid = nullableString("sdpMid"),
            sdpMLineIndex = nullableInt("sdpMLineIndex"),
        )

        SignalingType.InputEvent -> error("input_event parsing is not supported on Android client")
        SignalingType.Error -> SignalingPayload.Error(
            code = getString("code"),
            message = getString("message"),
            recoverable = getBoolean("recoverable"),
        )
    }

private fun JSONObject.nullableString(name: String): String? =
    if (isNull(name)) null else getString(name)

private fun JSONObject.nullableInt(name: String): Int? =
    if (isNull(name)) null else getInt(name)

private fun RemoteInputEvent.toJsonObject(): JSONObject =
    when (this) {
        is RemoteInputEvent.Keyboard -> JSONObject()
            .put("kind", "keyboard")
            .put("keyCode", keyCode)
            .put("action", if (action == android.view.KeyEvent.ACTION_DOWN) "down" else "up")

        is RemoteInputEvent.MouseMove -> JSONObject()
            .put("kind", "mouse_move")
            .put("dx", dx)
            .put("dy", dy)
            .put("mode", mode.name.lowercase())

        is RemoteInputEvent.MouseButton -> JSONObject()
            .put("kind", "mouse_button")
            .put("button", buttonState.toMouseButtonName())
            .put(
                "action",
                if (action == android.view.MotionEvent.ACTION_BUTTON_PRESS) "down" else "up",
            )

        is RemoteInputEvent.MouseWheel -> JSONObject()
            .put("kind", "mouse_wheel")
            .put("deltaX", deltaX)
            .put("deltaY", deltaY)
    }

private fun Int.toMouseButtonName(): String =
    when {
        this and android.view.MotionEvent.BUTTON_PRIMARY != 0 -> "left"
        this and android.view.MotionEvent.BUTTON_SECONDARY != 0 -> "right"
        this and android.view.MotionEvent.BUTTON_TERTIARY != 0 -> "middle"
        this and android.view.MotionEvent.BUTTON_BACK != 0 -> "back"
        this and android.view.MotionEvent.BUTTON_FORWARD != 0 -> "forward"
        else -> "left"
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

    fun sendInput(event: RemoteInputEvent): SignalingMessage? {
        if (!event.isSingleUserAction()) return null

        return enqueue(SignalingType.InputEvent, SignalingPayload.Input(event))
    }

    fun sendOffer(sdp: String): SignalingMessage? = sendSessionDescription(SignalingType.Offer, sdp)

    fun sendAnswer(sdp: String): SignalingMessage? = sendSessionDescription(SignalingType.Answer, sdp)

    fun sendIce(
        candidate: String,
        sdpMid: String?,
        sdpMLineIndex: Int?,
    ): SignalingMessage? {
        if (candidate.isBlank()) return null

        return enqueue(
            SignalingType.Ice,
            SignalingPayload.Ice(
                candidate = candidate,
                sdpMid = sdpMid,
                sdpMLineIndex = sdpMLineIndex,
            ),
        )
    }

    fun snapshotOutbox(): List<SignalingMessage> = outbox.toList()

    private fun sendSessionDescription(type: SignalingType, sdp: String): SignalingMessage? {
        if (sdp.isBlank()) return null

        return enqueue(type, SignalingPayload.SessionDescription(sdp))
    }

    private fun enqueue(type: SignalingType, payload: SignalingPayload): SignalingMessage {
        val message = SignalingMessage(
            type = type,
            requestId = UUID.randomUUID().toString(),
            payload = payload,
        )
        outbox += message
        return message
    }
}

private fun sha256(value: String): String {
    val digest = MessageDigest.getInstance("SHA-256").digest(value.toByteArray(Charsets.UTF_8))
    return digest.joinToString(separator = "") { byte -> "%02x".format(byte) }
}
