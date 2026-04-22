export interface ConnectionRecord {
  id: string;
  alias: string;
  address: string;
  fingerprint: string;
  lastConnected: number;
  connectionCount: number;
  icon: string;
  trustLevel: "trusted" | "ask" | "blocked";
}

const STORAGE_KEY = "vibe-remote-connection-history";

function loadHistory(): ConnectionRecord[] {
  if (typeof window === "undefined") return [];
  try {
    const stored = localStorage.getItem(STORAGE_KEY);
    return stored ? JSON.parse(stored) : [];
  } catch {
    return [];
  }
}

function saveHistory(records: ConnectionRecord[]) {
  if (typeof window === "undefined") return;
  localStorage.setItem(STORAGE_KEY, JSON.stringify(records));
}

let history = $state<ConnectionRecord[]>(loadHistory());

export function useConnectionHistory() {
  return history;
}

export function addToHistory(record: Omit<ConnectionRecord, "id" | "lastConnected" | "connectionCount">) {
  const existing = history.find(r => r.fingerprint === record.fingerprint);
  if (existing) {
    existing.lastConnected = Date.now();
    existing.connectionCount++;
    existing.address = record.address;
  } else {
    history.push({
      ...record,
      id: crypto.randomUUID(),
      lastConnected: Date.now(),
      connectionCount: 1,
    });
  }
  saveHistory([...history]);
}

export function removeFromHistory(id: string) {
  history = history.filter(r => r.id !== id);
  saveHistory(history);
}

export function updateTrustLevel(id: string, trustLevel: ConnectionRecord["trustLevel"]) {
  const record = history.find(r => r.id === id);
  if (record) {
    record.trustLevel = trustLevel;
    saveHistory([...history]);
  }
}