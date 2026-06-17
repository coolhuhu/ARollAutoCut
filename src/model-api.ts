import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";

export type ModelState = "missing" | "invalid" | "ready";

export interface ModelStatus {
  state: ModelState;
  directory: string;
  issues: string[];
}

export interface ModelDownloadProgress {
  stage: "downloading" | "extracting" | "verifying" | "complete";
  downloadedBytes: number;
  totalBytes: number | null;
  percent: number;
  message: string;
}

export interface VadSettings {
  minSilenceDuration: number;
  minSpeechDuration: number;
  maxSpeechDuration: number;
}

function isTauriRuntime(): boolean {
  return "__TAURI_INTERNALS__" in window;
}

export async function getModelStatus(): Promise<ModelStatus> {
  if (!isTauriRuntime()) {
    return {
      state: "missing",
      directory: "Tauri App Data/models",
      issues: ["请在 Tauri App 中配置 SenseVoice 模型"],
    };
  }
  return invoke<ModelStatus>("get_model_status");
}

export async function chooseModelDirectory(): Promise<ModelStatus | null> {
  if (!isTauriRuntime()) {
    return null;
  }

  const directory = await open({
    directory: true,
    multiple: false,
    title: "选择 SenseVoice 模型目录",
  });
  if (typeof directory !== "string") {
    return null;
  }

  return invoke<ModelStatus>("select_model_directory", { directory });
}

export async function downloadModel(
  onProgress: (progress: ModelDownloadProgress) => void,
): Promise<ModelStatus> {
  if (!isTauriRuntime()) {
    throw new Error("请在 Tauri App 中下载 SenseVoice 模型");
  }

  const unlisten = await listen<ModelDownloadProgress>(
    "model-download-progress",
    (event) => onProgress(event.payload),
  );
  try {
    return await invoke<ModelStatus>("download_model");
  } finally {
    unlisten();
  }
}

export async function cancelModelDownload(): Promise<boolean> {
  if (!isTauriRuntime()) {
    return false;
  }
  return invoke<boolean>("cancel_model_download");
}

export async function getVadSettings(): Promise<VadSettings> {
  if (!isTauriRuntime()) {
    return {
      minSilenceDuration: 0.5,
      minSpeechDuration: 0.25,
      maxSpeechDuration: 20,
    };
  }
  return invoke<VadSettings>("get_vad_settings");
}

export async function saveVadSettings(
  settings: VadSettings,
): Promise<VadSettings> {
  if (!isTauriRuntime()) {
    return settings;
  }
  return invoke<VadSettings>("save_vad_settings", { settings });
}

export async function resetVadSettings(): Promise<VadSettings> {
  if (!isTauriRuntime()) {
    return {
      minSilenceDuration: 0.5,
      minSpeechDuration: 0.25,
      maxSpeechDuration: 20,
    };
  }
  return invoke<VadSettings>("reset_vad_settings");
}

export function formatDownloadBytes(
  downloadedBytes: number,
  totalBytes: number | null,
): string {
  const downloaded = formatBytes(downloadedBytes);
  return totalBytes && totalBytes > 0
    ? `${downloaded} / ${formatBytes(totalBytes)}`
    : downloaded;
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) {
    return `${bytes} B`;
  }
  const units = ["KB", "MB", "GB"];
  let value = bytes / 1024;
  let unit = units[0];
  for (let index = 1; index < units.length && value >= 1024; index += 1) {
    value /= 1024;
    unit = units[index];
  }
  return `${value.toFixed(value >= 100 ? 0 : 1)} ${unit}`;
}
