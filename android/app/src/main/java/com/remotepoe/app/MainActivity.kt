package com.remotepoe.app

import android.os.Bundle
import android.view.InputDevice
import android.view.KeyEvent
import android.view.MotionEvent
import android.view.WindowInsets
import android.view.WindowInsetsController
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.background
import androidx.compose.foundation.focusable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.ExperimentalComposeUiApi
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.input.pointer.pointerInteropFilter
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.viewinterop.AndroidView
import com.remotepoe.app.remote.AndroidWebRtcPeerConnectionGateway
import com.remotepoe.app.remote.ConnectionState
import com.remotepoe.app.remote.PointerMode
import com.remotepoe.app.remote.RemoteClient
import com.remotepoe.app.remote.RemoteInputEvent
import com.remotepoe.app.remote.StreamConfig
import kotlin.math.roundToInt
import org.webrtc.EglBase
import org.webrtc.RendererCommon
import org.webrtc.SurfaceViewRenderer
import org.webrtc.VideoTrack

class MainActivity : ComponentActivity() {
    private lateinit var remoteClient: RemoteClient

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        remoteClient = RemoteClient(
            peerConnection = AndroidWebRtcPeerConnectionGateway(applicationContext),
        )
        enterImmersiveMode()

        setContent {
            RemotePoeApp(remoteClient)
        }
    }

    override fun onWindowFocusChanged(hasFocus: Boolean) {
        super.onWindowFocusChanged(hasFocus)
        if (hasFocus) enterImmersiveMode()
    }

    private fun enterImmersiveMode() {
        window.insetsController?.let { controller ->
            controller.hide(WindowInsets.Type.statusBars() or WindowInsets.Type.navigationBars())
            controller.systemBarsBehavior =
                WindowInsetsController.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
        }
    }
}

@Composable
private fun RemotePoeApp(remoteClient: RemoteClient) {
    var host by remember { mutableStateOf("100.x.x.x:7443") }
    var password by remember { mutableStateOf("") }
    var state by remember { mutableStateOf(ConnectionState.Disconnected) }
    var errorMessage by remember { mutableStateOf<String?>(null) }
    var remoteVideoTrack by remember { mutableStateOf<VideoTrack?>(null) }
    var streamConfig by remember { mutableStateOf(StreamConfig.default1080p60()) }

    DisposableEffect(remoteClient) {
        remoteClient.onConnectionChanged = { nextState, message ->
            state = nextState
            errorMessage = message
        }
        remoteClient.onRemoteVideoTrack = { track ->
            remoteVideoTrack = track
        }
        onDispose {
            remoteClient.onConnectionChanged = null
            remoteClient.onRemoteVideoTrack = null
        }
    }

    MaterialTheme {
        Surface(modifier = Modifier.fillMaxSize(), color = Color.Black) {
            if (state == ConnectionState.Connected) {
                PlayerScreen(
                    remoteVideoTrack = remoteVideoTrack,
                    onDisconnect = {
                        remoteClient.disconnect()
                        state = ConnectionState.Disconnected
                        errorMessage = null
                        remoteVideoTrack = null
                    },
                    onInput = remoteClient::sendInput,
                )
            } else {
                ConnectScreen(
                    host = host,
                    password = password,
                    streamConfig = streamConfig,
                    state = state,
                    errorMessage = errorMessage,
                    onHostChange = { host = it },
                    onPasswordChange = { password = it },
                    onStreamConfigChange = { streamConfig = it },
                    onConnect = {
                        state = ConnectionState.Connecting
                        errorMessage = null
                        state = remoteClient.connect(host, password, streamConfig)
                    },
                )
            }
        }
    }
}

@Composable
private fun ConnectScreen(
    host: String,
    password: String,
    streamConfig: StreamConfig,
    state: ConnectionState,
    errorMessage: String?,
    onHostChange: (String) -> Unit,
    onPasswordChange: (String) -> Unit,
    onStreamConfigChange: (StreamConfig) -> Unit,
    onConnect: () -> Unit,
) {
    Column(
        modifier = Modifier
            .fillMaxSize()
            .background(Color(0xFF101318))
            .padding(24.dp),
        verticalArrangement = Arrangement.Center,
    ) {
        Text("遠端 POE", color = Color.White, fontSize = 30.sp)
        Text("連到 Windows Host，透過 LAN、Tailscale 或 ZeroTier 遊玩。", color = Color(0xFFB7C0CF))

        OutlinedTextField(
            modifier = Modifier
                .fillMaxWidth()
                .padding(top = 24.dp),
            value = host,
            onValueChange = onHostChange,
            label = { Text("Host 位址") },
            singleLine = true,
        )
        OutlinedTextField(
            modifier = Modifier
                .fillMaxWidth()
                .padding(top = 12.dp),
            value = password,
            onValueChange = onPasswordChange,
            label = { Text("配對密碼") },
            singleLine = true,
            visualTransformation = PasswordVisualTransformation(),
        )
        Row(
            modifier = Modifier.padding(top = 12.dp),
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Button(onClick = { onStreamConfigChange(StreamConfig.default1080p60()) }) {
                Text("1080p60")
            }
            Button(onClick = { onStreamConfigChange(StreamConfig.fallback720p60()) }) {
                Text("720p60")
            }
        }
        Text(
            modifier = Modifier.padding(top = 8.dp),
            text = "${streamConfig.width}x${streamConfig.height}@${streamConfig.fps} ${streamConfig.bitrateKbps}kbps",
            color = Color(0xFFB7C0CF),
        )
        Text(
            modifier = Modifier.padding(top = 12.dp),
            text = state.statusText(errorMessage),
            color = if (state == ConnectionState.Failed) Color(0xFFFFB4AB) else Color(0xFFB7C0CF),
        )
        Button(
            modifier = Modifier.padding(top = 20.dp),
            enabled = state != ConnectionState.Connecting,
            onClick = onConnect,
        ) {
            Text(if (state == ConnectionState.Connecting) "連線中" else "連線")
        }
    }
}

@Composable
@OptIn(ExperimentalComposeUiApi::class)
private fun PlayerScreen(
    remoteVideoTrack: VideoTrack?,
    onDisconnect: () -> Unit,
    onInput: (RemoteInputEvent) -> Unit,
) {
    val focusRequester = remember { FocusRequester() }
    var lastMouseX by remember { mutableStateOf<Float?>(null) }
    var lastMouseY by remember { mutableStateOf<Float?>(null) }

    LaunchedEffect(Unit) {
        focusRequester.requestFocus()
    }

    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(Color.Black)
            .focusRequester(focusRequester)
            .focusable()
            .onPreviewKeyEvent { keyEvent ->
                val action = when (keyEvent.type) {
                    KeyEventType.KeyDown -> KeyEvent.ACTION_DOWN
                    KeyEventType.KeyUp -> KeyEvent.ACTION_UP
                    else -> return@onPreviewKeyEvent false
                }
                onInput(RemoteInputEvent.Keyboard(keyEvent.key.keyCode.toInt(), action))
                true
            }
            .pointerInteropFilter { motionEvent ->
                val nextPosition = forwardPointerEvent(
                    event = motionEvent,
                    previousMouseX = lastMouseX,
                    previousMouseY = lastMouseY,
                    onInput = onInput,
                )
                lastMouseX = nextPosition?.first
                lastMouseY = nextPosition?.second
                true
            },
    ) {
        RemoteVideoSurface(remoteVideoTrack = remoteVideoTrack)

        Row(
            modifier = Modifier
                .align(Alignment.TopEnd)
                .padding(12.dp),
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Button(onClick = onDisconnect) {
                Text("斷線")
            }
        }
    }
}

@Composable
private fun RemoteVideoSurface(remoteVideoTrack: VideoTrack?) {
    val eglBase = remember { EglBase.create() }
    var renderer by remember { mutableStateOf<SurfaceViewRenderer?>(null) }

    AndroidView(
        modifier = Modifier.fillMaxSize(),
        factory = { context ->
            SurfaceViewRenderer(context).apply {
                init(eglBase.eglBaseContext, null)
                setScalingType(RendererCommon.ScalingType.SCALE_ASPECT_FIT)
                setEnableHardwareScaler(true)
                renderer = this
            }
        },
        update = { view ->
            renderer = view
        },
    )

    DisposableEffect(remoteVideoTrack, renderer) {
        val activeRenderer = renderer
        if (activeRenderer != null && remoteVideoTrack != null) {
            remoteVideoTrack.addSink(activeRenderer)
        }

        onDispose {
            if (activeRenderer != null && remoteVideoTrack != null) {
                remoteVideoTrack.removeSink(activeRenderer)
            }
        }
    }

    DisposableEffect(Unit) {
        onDispose {
            renderer?.release()
            eglBase.release()
        }
    }
}

private fun ConnectionState.statusText(errorMessage: String?): String =
    when (this) {
        ConnectionState.Disconnected -> "尚未連線"
        ConnectionState.Connecting -> "正在連到 Host signaling"
        ConnectionState.Connected -> "已連線"
        ConnectionState.Failed -> errorMessage ?: "連線失敗"
    }

private fun forwardPointerEvent(
    event: MotionEvent,
    previousMouseX: Float?,
    previousMouseY: Float?,
    onInput: (RemoteInputEvent) -> Unit,
): Pair<Float, Float>? {
    val isMouse = event.source and InputDevice.SOURCE_MOUSE == InputDevice.SOURCE_MOUSE

    if (isMouse) {
        return forwardMouseEvent(event, previousMouseX, previousMouseY, onInput)
    }

    forwardTouchFallbackEvent(event, onInput)
    return null
}

private fun forwardMouseEvent(
    event: MotionEvent,
    previousMouseX: Float?,
    previousMouseY: Float?,
    onInput: (RemoteInputEvent) -> Unit,
): Pair<Float, Float>? {
    when (event.actionMasked) {
        MotionEvent.ACTION_MOVE,
        MotionEvent.ACTION_HOVER_MOVE -> {
            if (previousMouseX != null && previousMouseY != null) {
                val dx = (event.x - previousMouseX).roundToInt()
                val dy = (event.y - previousMouseY).roundToInt()
                if (dx != 0 || dy != 0) {
                    onInput(RemoteInputEvent.MouseMove(dx, dy, PointerMode.Relative))
                }
            }
            return event.x to event.y
        }

        MotionEvent.ACTION_BUTTON_PRESS,
        MotionEvent.ACTION_BUTTON_RELEASE -> {
            onInput(RemoteInputEvent.MouseButton(event.changedButton(), event.actionMasked))
            return event.x to event.y
        }

        MotionEvent.ACTION_SCROLL -> {
            onInput(
                RemoteInputEvent.MouseWheel(
                    deltaX = event.getAxisValue(MotionEvent.AXIS_HSCROLL).roundToInt(),
                    deltaY = event.getAxisValue(MotionEvent.AXIS_VSCROLL).roundToInt(),
                ),
            )
            return event.x to event.y
        }

        MotionEvent.ACTION_HOVER_EXIT,
        MotionEvent.ACTION_CANCEL -> return null
    }

    return previousMouseX?.let { x -> previousMouseY?.let { y -> x to y } }
}

private fun forwardTouchFallbackEvent(
    event: MotionEvent,
    onInput: (RemoteInputEvent) -> Unit,
) {
    when (event.actionMasked) {
        MotionEvent.ACTION_DOWN -> {
            onInput(RemoteInputEvent.MouseMove(event.x.roundToInt(), event.y.roundToInt(), PointerMode.Absolute))
            onInput(RemoteInputEvent.MouseButton(MotionEvent.BUTTON_PRIMARY, MotionEvent.ACTION_BUTTON_PRESS))
        }

        MotionEvent.ACTION_MOVE -> {
            onInput(RemoteInputEvent.MouseMove(event.x.roundToInt(), event.y.roundToInt(), PointerMode.Absolute))
        }

        MotionEvent.ACTION_UP,
        MotionEvent.ACTION_CANCEL -> {
            onInput(RemoteInputEvent.MouseButton(MotionEvent.BUTTON_PRIMARY, MotionEvent.ACTION_BUTTON_RELEASE))
        }
    }
}

private fun MotionEvent.changedButton(): Int =
    if (actionButton != 0) actionButton else buttonState
