# 遠端 POE v1 協定

## 連線

- Android 連到 `ws://<host>:7443/signaling`。
- 外網預設依賴 Tailscale 或 ZeroTier，不提供公開 relay。
- `wss://` 可作為後續部署選項；目前假設 VPN/LAN 已提供傳輸保護。

## 訊息格式

```json
{
  "type": "auth",
  "requestId": "client-request-id",
  "payload": {}
}
```

支援 type：

- `auth`
- `device_info`
- `stream_config`
- `offer`
- `answer`
- `ice`
- `input_event`
- `error`

## Auth

```json
{
  "deviceId": "android-device-id",
  "deviceName": "Android tablet",
  "publicKey": "base64-public-key",
  "passwordHash": "sha256-password"
}
```

Host 先檢查信任裝置；若未信任，密碼正確才加入信任清單並保存。

## Stream Config

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

支援：

- 1920x1080 60 FPS H.264
- 1280x720 60 FPS H.264

## WebRTC

Android 送 `offer`，Host 回 `answer`。Host 會等待 ICE gathering 完成後再回 answer，讓 SDP 內包含可用 candidate。後續 Android `ice` 會送回 Host。

輸入事件優先走 WebRTC `control` data channel；若 data channel 尚未開啟，Android fallback 到 WebSocket `input_event`。

## Input Event

Keyboard：

```json
{
  "kind": "keyboard",
  "keyCode": 87,
  "action": "down"
}
```

`keyCode` 使用 Windows virtual key。Android 端會把 Android keyCode 轉換成 Windows virtual key。

Mouse button：

```json
{
  "kind": "mouse_button",
  "button": "left",
  "action": "down"
}
```

Mouse move：

```json
{
  "kind": "mouse_move",
  "dx": 4,
  "dy": -2,
  "mode": "relative"
}
```

Mouse wheel：

```json
{
  "kind": "mouse_wheel",
  "deltaX": 0,
  "deltaY": 120
}
```

限制：不做多鍵巨集；一個使用者動作只送一個鍵鼠事件。
