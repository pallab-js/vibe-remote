import { invoke } from "@tauri-apps/api/core";

export interface SessionState {
  active: boolean;
  mode: "local" | "host" | "client" | "disconnected";
  connectionId: string;
  peerId: string;
  peerName: string;
  fps: number;
  latency: number;
  bytesSent: number;
  bytesReceived: number;
  framesSent: number;
}

let session = $state<SessionState>({
  active: false,
  mode: "disconnected",
  connectionId: "",
  peerId: "",
  peerName: "",
  fps: 0,
  latency: 0,
  bytesSent: 0,
  bytesReceived: 0,
  framesSent: 0,
});

export function useSession() {
  return session;
}

export async function updateStats() {
  try {
    const stats = await invoke("get_session_stats") as any;
    session.fps = 0;
    session.latency = Number(stats.latency_ms || 0);
    session.bytesSent = Number(stats.bytes_sent || 0);
    session.bytesReceived = Number(stats.bytes_received || 0);
    session.framesSent = Number(stats.frames_sent || 0);
    session.active = stats.active as boolean;
  } catch (e) {
    console.error("Failed to update stats:", e);
  }
}

export function resetSession() {
  session.active = false;
  session.mode = "disconnected";
  session.connectionId = "";
  session.peerId = "";
  session.peerName = "";
  session.fps = 0;
  session.latency = 0;
}

export async function startHost(port: number, displayIndex: number) {
  try {
    await invoke("start_server", { port });
    await invoke("start_remote_stream", { displayIndex });
    session.mode = "host";
    session.active = true;
  } catch (e) {
    throw e;
  }
}

export async function connectToPeer(host: string, port: number) {
  try {
    await invoke("connect_remote", {
      params: { host, port, serverFingerprint: null }
    });
    session.mode = "client";
    session.active = true;
    session.connectionId = `${host}:${port}`;
  } catch (e) {
    throw e;
  }
}