# 遠端 POE

個人自用的 Android 遠端遊玩 POE 1 系統。Android app 連到 Windows PC 上的 Tauri host，目標是透過 WebRTC 串流畫面，並優先轉送 Android 外接鍵盤與滑鼠事件來操作 PC。

## 範圍

- Host: Windows + Tauri UI + Rust backend。
- Android: Kotlin + Jetpack Compose，全螢幕沉浸式玩家端。
- 網路: LAN 或 Tailscale/ZeroTier VPN，不自建公開帳號、NAT 穿透或 relay。
- 安全: 固定密碼配對搭配信任裝置清單，signaling payload 維持可驗證形狀。
- 輸入: 一次使用者動作只對應一次鍵鼠事件，不做多鍵巨集。

## 專案結構

```text
android/          Android Kotlin/Compose app
host/             Tauri Windows host shell
host/host-core/   Rust 共用設定、配對、stream、input、signaling model
host/signaling-server/
                  WebSocket signaling server、event log、input injector
host/src-tauri/   Tauri commands 與 host app state
shared/           協定與驗收文件
```

## 目前狀態

- `host/host-core` 已有配對、信任裝置、stream config、input event 與 signaling model。
- `host/signaling-server` 已可處理 `auth`、`device_info`、`stream_config`、`input_event`、`offer`、`answer`、`ice`。
- Host 已提供 WebSocket signaling server，並在 UI 顯示最近事件、輸入記錄與 WebRTC offer/answer/ice 狀態。
- Host 收到 Android WebRTC offer 後，會透過可替換的 peer gateway skeleton 回傳 answer 與 host ICE；目前是 recording/stub backend，尚未接實際 media track。
- Windows build 已接 `SendInput + recording` backend，且 `SendInput` 只在前景視窗標題符合 POE 時執行；非 Windows build 使用 recording backend。
- Android app 已有連線頁、WebSocket transport、基礎沉浸式畫面、外接鍵鼠 mapping、input event 傳送，以及 WebRTC peer connection skeleton。
- Android WebSocket 連上後會建立本地 peer connection、產生 offer、送出本地 ICE，並可套用 host 回傳的 answer/ICE。
- Android 輸入事件會優先走 WebRTC `control` data channel；若 data channel 尚未開啟或送出失敗，會 fallback 到 WebSocket `input_event`。
- 尚未完成實際 Windows 畫面擷取、H.264 編碼與 WebRTC media track。

## 開發指令

Rust host:

```powershell
rtk cargo test -p host-core --offline
rtk cargo test -p signaling-server --offline
rtk cargo check -p remote-poe-host --offline
```

Tauri frontend:

```powershell
cd host
rtk npm.cmd run typecheck
```

Android:

```powershell
cd android
.\gradlew.bat :app:assembleDebug --console=plain --warning-mode=summary
.\gradlew.bat :app:testDebugUnitTest --console=plain --warning-mode=summary
```

Android build 需要 Android SDK、JDK 17+ 與 Gradle wrapper 可用。本 repo 目前位於非 ASCII 路徑，Android unit test 會先把 test classes 複製到 `%USERPROFILE%\.gradle\remote-poe\android-debug-unit-test`，避免 Gradle test worker classpath 編碼問題。
