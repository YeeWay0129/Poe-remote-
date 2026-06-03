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
};

function App() {
  const [status, setStatus] = useState<HostStatus | null>(null);
  const [error, setError] = useState<string | null>(null);

  async function refreshStatus() {
    try {
      setStatus(await invoke<HostStatus>("host_status"));
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

  async function toggleStreaming() {
    await invoke(status?.streaming ? "stop_streaming" : "start_streaming");
    await refreshStatus();
  }

  async function toggleSignaling() {
    try {
      const nextStatus = await invoke<HostStatus>(
        status?.signalingRunning ? "stop_signaling" : "start_signaling",
      );
      setStatus(nextStatus);
      setError(null);
    } catch (err) {
      setError(String(err));
    }
  }

  const recentEvents = status?.signalingEvents.slice(-8).reverse() ?? [];
  const recentInputEvents = status?.inputEvents.slice(-8).reverse() ?? [];

  return (
    <main>
      <header>
        <h1>遠端 POE Host</h1>
        <p>
          Windows 主機負責配對、signaling、輸入注入與後續串流管線；Android
          端透過 LAN 或 VPN 位址連線。
        </p>
      </header>

      <section className="panel status-grid">
        <div>
          <span className="label">Signaling</span>
          <strong>{status?.signalingRunning ? "執行中" : "已停止"}</strong>
          <small>{status?.signalingEndpoint ?? "ws://0.0.0.0:7443/signaling"}</small>
        </div>
        <div>
          <span className="label">輸入後端</span>
          <strong>{status?.inputBackend ?? "載入中"}</strong>
        </div>
        <div>
          <span className="label">串流</span>
          <strong>{status?.streaming ? "執行中" : "已停止"}</strong>
        </div>
        <div>
          <span className="label">Media backend</span>
          <strong>{status?.mediaBackend ?? "recording capture + null h264 encoder"}</strong>
          <small>{status?.encodedFrames ?? 0} encoded frames</small>
        </div>
        <div>
          <span className="label">串流設定</span>
          <strong>{status?.streamLabel ?? "載入中"}</strong>
        </div>
        <div>
          <span className="label">信任裝置</span>
          <strong>{status?.trustedDevices ?? 0}</strong>
        </div>
        <div className="actions">
          <button onClick={toggleSignaling}>
            {status?.signalingRunning ? "停止 Signaling" : "啟動 Signaling"}
          </button>
          <button onClick={toggleStreaming}>
            {status?.streaming ? "停止串流" : "啟動串流"}
          </button>
        </div>
      </section>

      <section className="panel status-grid">
        <div>
          <span className="label">WebRTC phase</span>
          <strong>{status?.peerPhase ?? "idle"}</strong>
        </div>
        <div>
          <span className="label">WebRTC backend</span>
          <strong>{status?.peerBackend ?? "recording webrtc"}</strong>
        </div>
        <div>
          <span className="label">Offer SDP</span>
          <strong>{formatBytes(status?.lastOfferBytes)}</strong>
        </div>
        <div>
          <span className="label">Answer SDP</span>
          <strong>{formatBytes(status?.lastAnswerBytes)}</strong>
        </div>
        <div>
          <span className="label">ICE candidates</span>
          <strong>{status?.iceCandidates ?? 0}</strong>
        </div>
      </section>

      <section className="panel">
        <h2>最近 Signaling 事件</h2>
        {recentEvents.length === 0 ? (
          <p className="muted">尚未收到 Android 連線事件。</p>
        ) : (
          <ul className="event-list">
            {recentEvents.map((event, index) => (
              <li key={`${event}-${index}`}>{event}</li>
            ))}
          </ul>
        )}
      </section>

      <section className="panel">
        <h2>最近輸入事件</h2>
        {recentInputEvents.length === 0 ? (
          <p className="muted">尚未收到鍵盤或滑鼠輸入。</p>
        ) : (
          <ul className="event-list">
            {recentInputEvents.map((event, index) => (
              <li key={`${event}-${index}`}>{event}</li>
            ))}
          </ul>
        )}
      </section>

      <section className="panel">
        <h2>下一步</h2>
        <ul>
          <li>把 Android 的 offer/answer/ice API 接到實際 WebRTC peer connection。</li>
          <li>Host 串接 Windows Graphics Capture、H.264 編碼與 WebRTC media track。</li>
          <li>補上持久化設定、系統匣與開機啟動選項。</li>
        </ul>
      </section>

      {error && <p className="error">{error}</p>}
    </main>
  );
}

function formatBytes(bytes: number | null | undefined): string {
  if (!bytes) return "尚未收到";

  return `${bytes} bytes`;
}

createRoot(document.getElementById("root")!).render(<App />);
