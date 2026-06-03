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
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.key.nativeKeyEvent
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.pointer.pointerInteropFilter
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.remotepoe.app.remote.ConnectionState
import com.remotepoe.app.remote.PointerMode
import com.remotepoe.app.remote.RemoteClient
import com.remotepoe.app.remote.RemoteInputEvent
import com.remotepoe.app.remote.StreamConfig

class MainActivity : ComponentActivity() {
    private val remoteClient = RemoteClient()

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
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
    val streamConfig = remember { StreamConfig.default1080p60() }

    MaterialTheme {
        Surface(modifier = Modifier.fillMaxSize(), color = Color.Black) {
            if (state == ConnectionState.Connected) {
                PlayerScreen(
                    streamConfig = streamConfig,
                    onDisconnect = {
                        remoteClient.disconnect()
                        state = ConnectionState.Disconnected
                    },
                    onInput = remoteClient::sendInput,
                )
            } else {
                ConnectScreen(
                    host = host,
                    password = password,
                    state = state,
                    onHostChange = { host = it },
                    onPasswordChange = { password = it },
                    onConnect = {
                        state = ConnectionState.Connecting
                        state = remoteClient.connect(host, password)
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
    state: ConnectionState,
    onHostChange: (String) -> Unit,
    onPasswordChange: (String) -> Unit,
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
        Text("輸入 Windows host 的 LAN 或 Tailscale/ZeroTier 位址。", color = Color(0xFFB7C0CF))

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
private fun PlayerScreen(
    streamConfig: StreamConfig,
    onDisconnect: () -> Unit,
    onInput: (RemoteInputEvent) -> Unit,
) {
    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(Color.Black)
            .focusable()
            .onPreviewKeyEvent { keyEvent ->
                onInput(RemoteInputEvent.Keyboard(keyEvent.nativeKeyEvent.keyCode, keyEvent.nativeKeyEvent.action))
                true
            }
            .pointerInteropFilter { motionEvent ->
                forwardPointerEvent(motionEvent, onInput)
                true
            },
    ) {
        Text(
            modifier = Modifier.align(Alignment.Center),
            text = "${streamConfig.width}x${streamConfig.height}@${streamConfig.fps} WebRTC 畫面",
            color = Color(0xFFB7C0CF),
        )

        Row(
            modifier = Modifier
                .align(Alignment.TopEnd)
                .padding(12.dp),
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Button(onClick = onDisconnect) {
                Text("中斷")
            }
        }
    }
}

private fun forwardPointerEvent(
    event: MotionEvent,
    onInput: (RemoteInputEvent) -> Unit,
) {
    val source = event.source
    val isMouse = source and InputDevice.SOURCE_MOUSE == InputDevice.SOURCE_MOUSE

    when (event.actionMasked) {
        MotionEvent.ACTION_MOVE -> {
            onInput(
                RemoteInputEvent.MouseMove(
                    dx = event.x.toInt(),
                    dy = event.y.toInt(),
                    mode = if (isMouse) PointerMode.Relative else PointerMode.Absolute,
                )
            )
        }

        MotionEvent.ACTION_BUTTON_PRESS,
        MotionEvent.ACTION_BUTTON_RELEASE -> {
            onInput(RemoteInputEvent.MouseButton(event.buttonState, event.actionMasked))
        }

        MotionEvent.ACTION_SCROLL -> {
            onInput(
                RemoteInputEvent.MouseWheel(
                    deltaX = event.getAxisValue(MotionEvent.AXIS_HSCROLL).toInt(),
                    deltaY = event.getAxisValue(MotionEvent.AXIS_VSCROLL).toInt(),
                )
            )
        }
    }
}
