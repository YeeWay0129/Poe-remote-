# 遠端 POE

個人自用的 Android 遠端遊玩 POE 1 系統。Android App 連到 Windows Host，透過 WebRTC 收畫面，並把外接鍵盤滑鼠事件送回 PC。

## 目前內容

- `android/`: Kotlin + Jetpack Compose App。
- `host/`: Tauri Windows Host，Rust backend 負責 signaling、WebRTC、媒體 pipeline、輸入注入。
- `shared/`: 協定文件。

## 已完成

- Android 連線頁、沉浸式播放器、WebRTC video renderer。
- Android 外接鍵盤滑鼠事件轉送，鍵盤會轉成 Windows virtual key。
- Android 裝置身份會保存，使用 AndroidKeyStore 產生公鑰。
- Host WebSocket signaling、配對密碼、信任裝置、設定持久化。
- Host WebRTC answer、ICE gathering、H.264 video track、control data channel。
- Host Windows GDI 擷取與 Media Foundation H.264 encoder feature。
- Host Windows SendInput，並限制 POE 視窗在前景時才注入。
- Host 系統匣、開機啟動切換、串流設定 UI、事件記錄。

## 使用方式

1. Windows PC 安裝並登入 Tailscale 或 ZeroTier。
2. 啟動 Host，按「啟動 Signaling」和「啟動串流」。
3. Android 輸入 `HostIP:7443`，密碼預設是 `remote-poe`。
4. 第一次連線成功後，Host 會信任該 Android 裝置。

## 常用檢查

```powershell
rtk cargo test --workspace --offline
rtk cargo check -p remote-poe-host --features real-webrtc,media-windows-capture,media-windows-mf-h264 --offline
```

```powershell
cd host
rtk npm.cmd run typecheck
rtk npm.cmd run build:web
```

```powershell
cd android
rtk .\gradlew.bat :app:testDebugUnitTest --console=plain --warning-mode=summary
rtk .\gradlew.bat :app:assembleDebug --console=plain --warning-mode=summary
```

## 尚需實機驗收

- Windows + Android 真機端到端連線。
- POE 1 登入、移動、技能、背包、地圖操作。
- 1080p60 LAN/VPN 延遲與穩定性。
- 不同顯卡的 Media Foundation H.264 相容性。
