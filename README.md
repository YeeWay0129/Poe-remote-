# 遠端 POE

個人自用的 Android 遠端遊玩 POE 系統。Android app 連線到 Windows PC 上的 Tauri host，透過 WebRTC 串流畫面，並優先轉送 Android 外接鍵盤滑鼠事件來操作 PC。

## 目標

- Host：Windows + Tauri UI + Rust backend。
- Android：Kotlin + Jetpack Compose，全螢幕沉浸式串流。
- 網路：LAN 或 Tailscale/ZeroTier VPN，不自建 relay 或公開帳號系統。
- 安全：固定配對密碼 + 裝置信任清單，不採純密碼裸連。
- 輸入：外接鍵盤滑鼠優先；觸控只做 fallback 與連線控制。

## 專案結構

```text
android/          Android Kotlin/Compose app
host/             Tauri Windows host shell
host/host-core/   可測的 Rust host 核心模型
shared/           Signaling、stream、input 協定文件
```

## 目前狀態

這是可延伸的初始實作骨架：

- `host/host-core` 已有設定、配對、輸入事件與串流設定模型，以及單元測試。
- `host/signaling-server` 已有 WebSocket signaling frame 處理、配對、串流設定更新、輸入事件驗證。
- `host/src-tauri` 已有 Tauri command 邊界，用於後續接 Windows 擷取、WebRTC、SendInput。
- `android/app` 已有 Compose 連線畫面、沉浸式播放器骨架、外接鍵鼠事件轉送與 WebSocket transport 邊界。
- `shared/protocol.md` 固定首版訊息與行為邊界。

真正的 H.264 硬體編碼、WebRTC media track、Windows SendInput 注入仍是下一階段實作項目。

## 驗證

目前可先驗證 Rust host core：

```powershell
rtk cargo test -p host-core
rtk cargo test -p signaling-server
cd android
.\gradlew.bat :app:assembleDebug
```

Android/Tauri 完整建置需要安裝 Android SDK、JDK 17+、Node 依賴與 Tauri toolchain。
