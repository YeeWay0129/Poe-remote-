import React, { useEffect, useState } from "react";
import { createRoot } from "react-dom/client";
import { invoke } from "@tauri-apps/api/core";
import "./styles.css";

type HostStatus = {
  streaming: boolean;
  trustedDevices: number;
  streamLabel: string;
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
  }, []);

  async function toggleStreaming() {
    await invoke(status?.streaming ? "stop_streaming" : "start_streaming");
    await refreshStatus();
  }

  return (
    <main>
      <header>
        <h1>遠端 POE Host</h1>
        <p>Windows 主機端，負責串流 POE 畫面與接收 Android 鍵鼠輸入。</p>
      </header>

      <section className="panel">
        <div>
          <span className="label">串流狀態</span>
          <strong>{status?.streaming ? "執行中" : "已停止"}</strong>
        </div>
        <div>
          <span className="label">串流設定</span>
          <strong>{status?.streamLabel ?? "讀取中"}</strong>
        </div>
        <div>
          <span className="label">信任裝置</span>
          <strong>{status?.trustedDevices ?? 0}</strong>
        </div>
        <button onClick={toggleStreaming}>
          {status?.streaming ? "停止串流" : "啟動串流"}
        </button>
      </section>

      <section className="panel">
        <h2>下一階段</h2>
        <ul>
          <li>接上 Windows Graphics Capture 與 H.264 硬體編碼。</li>
          <li>接上 WSS signaling 與 WebRTC media/data channel。</li>
          <li>把輸入事件轉成 SendInput。</li>
        </ul>
      </section>

      {error && <p className="error">{error}</p>}
    </main>
  );
}

createRoot(document.getElementById("root")!).render(<App />);
