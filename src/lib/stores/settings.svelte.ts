export interface Settings {
  displayName: string;
  defaultPort: number;
  quality: "low" | "medium" | "high" | "ultra";
  framerate: number;
  bitrate: number;
  adaptiveBitrate: boolean;
  autoStartServer: boolean;
  requireApproval: boolean;
  showOnboard: boolean;
}

const DEFAULT_SETTINGS: Settings = {
  displayName: "",
  defaultPort: 4567,
  quality: "high",
  framerate: 60,
  bitrate: 4000000,
  adaptiveBitrate: true,
  autoStartServer: false,
  requireApproval: false,
  showOnboard: true,
};

const STORAGE_KEY = "vibe-remote-settings";

function loadSettings(): Settings {
  if (typeof window === "undefined") return DEFAULT_SETTINGS;
  try {
    const stored = localStorage.getItem(STORAGE_KEY);
    return stored ? { ...DEFAULT_SETTINGS, ...JSON.parse(stored) } : DEFAULT_SETTINGS;
  } catch {
    return DEFAULT_SETTINGS;
  }
}

function saveSettings(s: Settings) {
  if (typeof window === "undefined") return;
  localStorage.setItem(STORAGE_KEY, JSON.stringify(s));
}

let settings = $state<Settings>(loadSettings());

export function useSettings() {
  return settings;
}

export function updateSettings(updates: Partial<Settings>) {
  settings = { ...settings, ...updates };
  saveSettings(settings);
}

export function resetSettings() {
  settings = { ...DEFAULT_SETTINGS };
  saveSettings(settings);
}