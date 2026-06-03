# 遠端 POE v1 協定

## 連線模型

Android app 連到 Windows host 的 signaling endpoint。LAN 與 VPN 使用相同流程：

- 手動輸入 `ws://<host>:7443/signaling`，或輸入 host/IP 後由 Android 補成 WebSocket URL。
- 外網預設依賴 Tailscale/ZeroTier IP。
- 首版不提供公開帳號、NAT 穿透或 relay。
- WSS/TLS 先保留為可部署選項，目前 host 以 VPN/LAN 的 plain WebSocket 為主。

## 訊息外框

所有 signaling 訊息共用下列外框：

```json
{
  "type": "auth",
  "requestId": "uuid-or-client-request-id",
  "payload": {}
}
```

Host 會驗證 `type` 與 `payload` 形狀是否相符，`requestId` 不可為空。

## 訊息類型

- `auth`: 固定密碼配對或信任裝置登入。
- `device_info`: Android 裝置與能力資訊。
- `stream_config`: 解析度、FPS、bitrate、codec。
- `offer`: WebRTC SDP offer。
- `answer`: WebRTC SDP answer。
- `ice`: ICE candidate。
- `input_event`: 使用者鍵鼠事件。
- `error`: 可回復或不可回復錯誤。

## Auth payload

```json
{
  "deviceId": "android-dev-id",
  "deviceName": "Android tablet",
  "publicKey": "device-public-key",
  "passwordHash": "sha256-password"
}
```

## Device info payload

```json
{
  "deviceId": "android-dev-id",
  "deviceName": "Android tablet",
  "appVersion": "0.1.0",
  "supportsExternalKeyboard": true,
  "supportsExternalMouse": true
}
```

## Stream config payload

```json
{
  "width": 1920,
  "height": 1080,
  "fps": 60,
  "bitrateKbps": 12000,
  "codec": "h264",
  "displayId": null
}
```

支援目標：

- 預設: 1920x1080、60 FPS、H.264。
- fallback: 1280x720、60 FPS、H.264。
- LAN 延遲目標小於 80ms。
- VPN 延遲目標小於 150ms。

## WebRTC payload

SDP offer/answer:

```json
{
  "sdp": "v=0\r\n..."
}
```

ICE candidate:

```json
{
  "candidate": "candidate:...",
  "sdpMid": "0",
  "sdpMLineIndex": 0
}
```

Host 目前會記錄最後收到的 offer SDP bytes、answer SDP bytes、ICE candidate 數量與最後的 `sdpMid`，供 UI 顯示 signaling 狀態。實際 peer connection 與 media track 仍是後續階段。

## Input event payload

鍵盤：

```json
{
  "kind": "keyboard",
  "keyCode": 87,
  "action": "down"
}
```

滑鼠按鍵：

```json
{
  "kind": "mouse_button",
  "button": "left",
  "action": "down"
}
```

滑鼠移動：

```json
{
  "kind": "mouse_move",
  "dx": 4,
  "dy": -2,
  "mode": "relative"
}
```

滑鼠滾輪：

```json
{
  "kind": "mouse_wheel",
  "deltaX": 0,
  "deltaY": 120
}
```

Android 系統攔截的按鍵不保證可轉送。POE 快捷操作不做多鍵巨集，維持一次使用者動作對應一次鍵鼠事件。
