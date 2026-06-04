import React, { useEffect, useState } from "react";
import { createRoot } from "react-dom/client";
import { invoke } from "@tauri-apps/api/core";
import "./styles.css";

type HostStatus = {
  streaming: boolean;
  signalingRunning: boolean;
  signalingEndpoint: string | null;
  signalingEvents: string[];
  inputEvents: string[];
  inputBackend: string;
  peerBackend: string;
  mediaBackend: string;
  capturedFrames: number;
  encodedFrames: number;
  lastCapturedBytes: number | null;
  lastEncodedBytes: number | null;
  trustedDevices: number;
  streamLabel: string;
  peerPhase: string;
  lastOfferBytes: number | null;
  lastAnswerBytes: number | null;
  iceCandidates: number;
  autostart: boolean;
};

type TrustedDevice = {
  deviceId: string;
  deviceName: string;
  publicKey: string;
};

function App() {
  const [status, setStatus] = useState<HostStatus | null>(null);
  const [trustedDevices, setTrustedDevices] = useState<TrustedDevice[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [pairingPassword, setPairingPassword] = useState("");
  const [streamPreset, setStreamPreset] = useState("1080p60");
  const [bitrateKbps, setBitrateKbps] = useState(12000);
  const [settingsMessage, setSettingsMessage] = useState<string | null>(null);

  async function refreshStatus() {
    try {
      const [nextStatus, nextDevices] = await Promise.all([
        invoke<HostStatus>("host_status"),
        invoke<TrustedDevice[]>("trusted_devices"),
      ]);
      setStatus(nextStatus);
      setTrustedDevices(nextDevices);
      setError(null);
    } catch (err) {
      setError(String(err));
    }
  }

  useEffect(() => {
    refreshStatus();
    const timer = window.setInterval(refreshStatus, 2000);
    return () => window.clearInterval(timer);
  }, []);

  async function runStatusCommand(command: string) {
    try {
      const nextStatus = await invoke<HostStatus>(command);
      setStatus(nextStatus);
      setError(null);
    } catch (err) {
      setError(String(err));
    }
  }

  async function toggleStreaming() {
    await runStatusCommand(status?.streaming ? "stop_streaming" : "start_streaming");
  }

  async function toggleSignaling() {
    await runStatusCommand(status?.signalingRunning ? "stop_signaling" : "start_signaling");
  }

  async function toggleAutostart() {
    try {
      const nextStatus = await invoke<HostStatus>("update_autostart", {
        request: { enabled: !status?.autostart },
      });
      setStatus(nextStatus);
      setSettingsMessage(nextStatus.autostart ? "已設定開機啟動。" : "已關閉開機啟動。");
      setError(null);
    } catch (err) {
      setError(String(err));
      setSettingsMessage(null);
    }
  }

  async function savePairingPassword() {
    if (!pairingPassword) {
      setError("請輸入配對密碼。");
      return;
    }

    try {
      const passwordHash = await sha256Hex(pairingPassword);
      const nextStatus = await invoke<HostStatus>("update_pairing_password", {
        request: { passwordHash },
      });
      setStatus(nextStatus);
      setPairingPassword("");
      setSettingsMessage("配對密碼已儲存。");
      setError(null);
    } catch (err) {
      setError(String(err));
      setSettingsMessage(null);
    }
  }

  async function revokeTrustedDevice(deviceId: string) {
    try {
      await invoke<boolean>("revoke_device", { deviceId });
      await refreshStatus();
    } catch (err) {
      setError(String(err));
    }
  }

  async function saveStreamConfig() {
    const preset =
      streamPreset === "720p60"
        ? { width: 1280, height: 720, fps: 60 }
        : { width: 1920, height: 1080, fps: 60 };

    try {
      const nextStatus = await invoke<HostStatus>("update_stream_config", {
        request: {
          ...preset,
          bitrateKbps,
        },
      });
      setStatus(nextStatus);
      setSettingsMessage("串流設定已儲存。");
      setError(null);
    } catch (err) {
      setError(String(err));
      setSettingsMessage(null);
    }
  }

  const recentEvents = status?.signalingEvents.slice(-8).reverse() ?? [];
  const recentInputEvents = status?.inputEvents.slice(-8).reverse() ?? [];

  return (
    <main>
      <header>
        <h1>遠端 POE Host</h1>
        <p>Windows 端背景程式，負責串流畫面、接收 Android 連線，並轉送鍵盤滑鼠操作。</p>
      </header>

      <section className="panel status-grid">
        <StatusItem
          label="Signaling"
          value={status?.signalingRunning ? "執行中" : "未啟動"}
          detail={status?.signalingEndpoint ?? "ws://0.0.0.0:7443/signaling"}
        />
        <StatusItem label="串流" value={status?.streaming ? "執行中" : "未啟動"} />
        <StatusItem label="串流設定" value={status?.streamLabel ?? "載入中"} />
        <StatusItem label="信任裝置" value={String(status?.trustedDevices ?? 0)} />
        <StatusItem label="開機啟動" value={status?.autostart ? "已啟用" : "已關閉"} />
        <div className="actions">
          <button onClick={toggleSignaling}>
            {status?.signalingRunning ? "停止 Signaling" : "啟動 Signaling"}
          </button>
          <button onClick={toggleStreaming}>{status?.streaming ? "停止串流" : "啟動串流"}</button>
          <button className="secondary" onClick={toggleAutostart}>
            {status?.autostart ? "關閉開機啟動" : "開啟開機啟動"}
          </button>
        </div>
      </section>

      <section className="panel status-grid">
        <StatusItem label="WebRTC 狀態" value={status?.peerPhase ?? "idle"} />
        <StatusItem label="WebRTC 後端" value={status?.peerBackend ?? "recording webrtc"} />
        <StatusItem label="媒體後端" value={status?.mediaBackend ?? "recording capture + null h264 encoder"} />
        <StatusItem label="Offer SDP" value={formatBytes(status?.lastOfferBytes)} />
        <StatusItem label="Answer SDP" value={formatBytes(status?.lastAnswerBytes)} />
        <StatusItem label="ICE 數量" value={String(status?.iceCandidates ?? 0)} />
        <StatusItem label="已編碼影格" value={String(status?.encodedFrames ?? 0)} />
        <StatusItem label="最後輸出大小" value={formatBytes(status?.lastEncodedBytes)} />
      </section>

      <section className="panel settings-panel">
        <h2>配對密碼</h2>
        <div className="settings-row">
          <input
            type="password"
            value={pairingPassword}
            onChange={(event) => setPairingPassword(event.currentTarget.value)}
            placeholder="輸入新的配對密碼"
          />
          <button onClick={savePairingPassword}>儲存</button>
        </div>
        <p className="muted">Android 第一次連線要輸入這組密碼；預設密碼是 remote-poe。</p>
        {settingsMessage && <p className="success">{settingsMessage}</p>}
      </section>

      <section className="panel settings-panel">
        <h2>串流設定</h2>
        <div className="settings-row">
          <select value={streamPreset} onChange={(event) => setStreamPreset(event.currentTarget.value)}>
            <option value="1080p60">1080p60</option>
            <option value="720p60">720p60</option>
          </select>
          <input
            type="number"
            min="1000"
            max="50000"
            step="500"
            value={bitrateKbps}
            onChange={(event) => setBitrateKbps(Number(event.currentTarget.value))}
          />
          <button onClick={saveStreamConfig}>套用</button>
        </div>
        <p className="muted">目前設定：{status?.streamLabel ?? "尚未讀取"}</p>
      </section>

      <section className="panel">
        <h2>信任裝置</h2>
        {trustedDevices.length === 0 ? (
          <p className="muted">目前沒有已配對裝置。</p>
        ) : (
          <ul className="device-list">
            {trustedDevices.map((device) => (
              <li key={device.deviceId}>
                <div>
                  <strong>{device.deviceName || device.deviceId}</strong>
                  <small>{device.deviceId}</small>
                </div>
                <button className="danger" onClick={() => revokeTrustedDevice(device.deviceId)}>
                  移除
                </button>
              </li>
            ))}
          </ul>
        )}
      </section>

      <section className="panel">
        <h2>最近 Signaling 事件</h2>
        <EventList events={recentEvents} emptyText="尚未收到 Android 連線事件。" />
      </section>

      <section className="panel">
        <h2>最近輸入事件</h2>
        <EventList events={recentInputEvents} emptyText="尚未收到外接鍵盤滑鼠事件。" />
      </section>

      {error && <p className="error">{error}</p>}
    </main>
  );
}

function StatusItem({ label, value, detail }: { label: string; value: string; detail?: string }) {
  return (
    <div>
      <span className="label">{label}</span>
      <strong>{value}</strong>
      {detail && <small>{detail}</small>}
    </div>
  );
}

function EventList({ events, emptyText }: { events: string[]; emptyText: string }) {
  if (events.length === 0) {
    return <p className="muted">{emptyText}</p>;
  }

  return (
    <ul className="event-list">
      {events.map((event, index) => (
        <li key={`${event}-${index}`}>{event}</li>
      ))}
    </ul>
  );
}

function formatBytes(bytes: number | null | undefined): string {
  if (!bytes) return "尚無資料";

  return `${bytes} bytes`;
}

async function sha256Hex(value: string): Promise<string> {
  const bytes = new TextEncoder().encode(value);
  const digest = await crypto.subtle.digest("SHA-256", bytes);
  return Array.from(new Uint8Array(digest))
    .map((byte) => byte.toString(16).padStart(2, "0"))
    .join("");
}

createRoot(document.getElementById("root")!).render(<App />);
