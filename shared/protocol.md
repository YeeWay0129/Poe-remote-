# 遠端 POE v1 協定

## 連線模型

Android app 使用手動輸入 host 位址連線到 Windows host。位址可以是 LAN IP，也可以是 Tailscale/ZeroTier IP。首版不提供公開帳號、NAT 穿透或 relay。

開發階段 signaling endpoint：

- 預設位址：`ws://<host>:7443/signaling`
- VPN/LAN 內使用 plain WebSocket，外網暴露不列入支援範圍。
- WSS/TLS 會在 host 安裝流程與憑證儲存完成後接上；不允許把 plain WebSocket 直接暴露到公開網路。
- Host UI 會顯示最近 signaling 事件；Android UI 只有在 WebSocket `onOpen` 後才進入播放器畫面。
- Host 目前先把有效的 `input_event` 送到 recording input injector，UI 會顯示最近輸入事件；Windows `SendInput` backend 會在下一階段替換這個 injector。

## 配對

1. Host 設定固定配對密碼。
2. Android 首次連線時送出裝置名稱、裝置公鑰與密碼驗證資料。
3. Host 驗證成功後把裝置公鑰加入信任清單。
4. 之後同一裝置必須使用已信任公鑰連線。

純固定密碼連線不列入 v1 支援範圍。

## Signaling 訊息

WebSocket/WSS signaling 訊息以 `type` 欄位區分：

- `auth`: 配對或信任裝置驗證。
- `device_info`: Android 裝置能力、app 版本、輸入能力。
- `stream_config`: 解析度、幀率、碼率、螢幕選擇。
- `offer`: WebRTC SDP offer。
- `answer`: WebRTC SDP answer。
- `ice`: ICE candidate。
- `input_event`: 鍵盤滑鼠事件。
- `error`: 可恢復或不可恢復錯誤。

所有訊息都必須包含：

```json
{
  "type": "auth",
  "requestId": "uuid-or-client-request-id",
  "payload": {}
}
```

`type` 必須和 `payload` 形狀一致。Host 收到訊息後要先檢查 `requestId` 不為空，再依 `type` 驗證 payload。

### Auth payload

```json
{
  "deviceId": "android-dev-id",
  "deviceName": "Android tablet",
  "publicKey": "device-public-key",
  "passwordHash": "sha256-password"
}
```

### Stream config payload

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

### Input event payload

```json
{
  "kind": "keyboard",
  "keyCode": 87,
  "action": "down"
}
```

滑鼠按鍵事件：

```json
{
  "kind": "mouse_button",
  "button": "left",
  "action": "down"
}
```

## 串流預設

- 預設：1920x1080、60 FPS、H.264。
- LAN 延遲目標：低於 80ms。
- VPN 延遲目標：低於 150ms。
- fallback：1280x720、60 FPS。

## 輸入事件邊界

首版只允許使用者一次動作對應一次鍵鼠事件：

- keyboard down/up
- mouse move
- mouse button down/up
- mouse wheel
- pointer mode switch

不提供錄製巨集、多鍵序列或自動化操作。Android 系統保留鍵可能無法完整攔截，實作需在 UI 中標示為不保證支援。
