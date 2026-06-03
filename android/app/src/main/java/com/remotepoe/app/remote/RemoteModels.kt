package com.remotepoe.app.remote

import android.view.KeyEvent
import android.view.MotionEvent

enum class ConnectionState {
    Disconnected,
    Connecting,
    Connected,
    Failed,
}

enum class PointerMode {
    Relative,
    Absolute,
}

data class StreamConfig(
    val width: Int,
    val height: Int,
    val fps: Int,
    val bitrateKbps: Int,
) {
    companion object {
        fun default1080p60() = StreamConfig(
            width = 1920,
            height = 1080,
            fps = 60,
            bitrateKbps = 12_000,
        )

        fun fallback720p60() = StreamConfig(
            width = 1280,
            height = 720,
            fps = 60,
            bitrateKbps = 6_000,
        )
    }
}

sealed interface RemoteInputEvent {
    data class Keyboard(
        val keyCode: Int,
        val action: Int,
    ) : RemoteInputEvent

    data class MouseMove(
        val dx: Int,
        val dy: Int,
        val mode: PointerMode,
    ) : RemoteInputEvent

    data class MouseButton(
        val buttonState: Int,
        val action: Int,
    ) : RemoteInputEvent

    data class MouseWheel(
        val deltaX: Int,
        val deltaY: Int,
    ) : RemoteInputEvent
}

fun RemoteInputEvent.isSingleUserAction(): Boolean =
    when (this) {
        is RemoteInputEvent.Keyboard ->
            action == KeyEvent.ACTION_DOWN || action == KeyEvent.ACTION_UP

        is RemoteInputEvent.MouseMove ->
            dx != 0 || dy != 0

        is RemoteInputEvent.MouseButton ->
            action == MotionEvent.ACTION_BUTTON_PRESS || action == MotionEvent.ACTION_BUTTON_RELEASE

        is RemoteInputEvent.MouseWheel ->
            deltaX != 0 || deltaY != 0
    }
