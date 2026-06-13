import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  confirm,
  open,
  save,
  type ConfirmDialogOptions,
} from "@tauri-apps/plugin-dialog";

export interface TranscriptSegment {
  id: number;
  startSample: number;
  endSample: number;
  sampleRate: number;
  originalText: string;
  editedText: string;
  retained: boolean;
}

export interface TranscriptionResult {
  sourcePath: string;
  sourceName: string;
  mediaKind: "audio" | "video";
  segments: TranscriptSegment[];
}

export interface TranscriptionProgress {
  stage: "preparing" | "vad" | "recognizing" | "complete";
  percent: number;
  message: string;
}

export interface ExportProgress {
  percent: number;
  message: string;
}

export interface ExportResult {
  mediaPath: string;
  subtitlePath: string;
}

function isTauriRuntime(): boolean {
  return "__TAURI_INTERNALS__" in window;
}

export async function chooseMediaFile(): Promise<string | null> {
  if (!isTauriRuntime()) {
    return null;
  }

  const path = await open({
    multiple: false,
    directory: false,
    title: "选择视频或音频文件",
    filters: [
      {
        name: "支持的音视频文件",
        extensions: ["mp4", "mov", "wav", "mp3", "m4a", "aac", "flac"],
      },
    ],
  });
  return typeof path === "string" ? path : null;
}

export async function transcribeMedia(
  path: string,
  onProgress: (progress: TranscriptionProgress) => void,
): Promise<TranscriptionResult> {
  if (!isTauriRuntime()) {
    throw new Error("请在 Tauri App 中执行语音识别");
  }

  const unlisten = await listen<TranscriptionProgress>(
    "transcription-progress",
    (event) => onProgress(event.payload),
  );
  try {
    return await invoke<TranscriptionResult>("transcribe_media", { path });
  } finally {
    unlisten();
  }
}

export async function cancelTranscription(): Promise<boolean> {
  if (!isTauriRuntime()) {
    return false;
  }
  return invoke<boolean>("cancel_transcription");
}

export async function chooseExportDestination(
  sourcePath: string,
): Promise<string | null> {
  if (!isTauriRuntime()) {
    return null;
  }
  const extension = sourcePath.split(".").pop()?.toLowerCase() ?? "";
  return save({
    title: "导出剪辑后的媒体",
    defaultPath: defaultExportName(sourcePath),
    filters: extension
      ? [{ name: `${extension.toUpperCase()} 文件`, extensions: [extension] }]
      : undefined,
  });
}

export async function exportEditedMedia(
  sourcePath: string,
  destinationPath: string,
  mediaKind: "audio" | "video",
  segments: TranscriptSegment[],
  onProgress: (progress: ExportProgress) => void,
): Promise<ExportResult> {
  if (!isTauriRuntime()) {
    throw new Error("请在 Tauri App 中导出媒体");
  }

  const unlisten = await listen<ExportProgress>("export-progress", (event) =>
    onProgress(event.payload),
  );
  try {
    return await invoke<ExportResult>("export_edited_media", {
      sourcePath,
      destinationPath,
      mediaKind,
      segments,
    });
  } finally {
    unlisten();
  }
}

export async function cancelExport(): Promise<boolean> {
  if (!isTauriRuntime()) {
    return false;
  }
  return invoke<boolean>("cancel_export");
}

export async function listenForMediaDrop(
  onDrop: (path: string) => void,
): Promise<UnlistenFn> {
  if (!isTauriRuntime()) {
    return () => {};
  }

  return getCurrentWindow().onDragDropEvent((event) => {
    if (event.payload.type === "drop" && event.payload.paths[0]) {
      onDrop(event.payload.paths[0]);
    }
  });
}

export async function listenForAppClose(
  hasUnfinishedWork: () => boolean,
): Promise<UnlistenFn> {
  if (!isTauriRuntime()) {
    return () => {};
  }

  const appWindow = getCurrentWindow();
  let confirmationOpen = false;
  return appWindow.onCloseRequested(async (event) => {
    if (confirmationOpen) {
      event.preventDefault();
      return;
    }

    confirmationOpen = true;
    try {
      await handleAppCloseRequest(
        event,
        hasUnfinishedWork(),
        confirm,
        () => appWindow.destroy(),
      );
    } finally {
      confirmationOpen = false;
    }
  });
}

type CloseRequestEvent = {
  preventDefault: () => void;
};

type ConfirmClose = (
  message: string,
  options: ConfirmDialogOptions,
) => Promise<boolean>;

export async function handleAppCloseRequest(
  event: CloseRequestEvent,
  hasUnfinishedWork: boolean,
  confirmClose: ConfirmClose,
  destroyWindow: () => Promise<void>,
): Promise<void> {
  event.preventDefault();
  const message = hasUnfinishedWork
    ? "当前操作尚未完成，关闭 App 将放弃本次编辑。是否继续？"
    : "是否关闭 ARollCut？";
  const confirmed = await confirmClose(message, {
    title: "关闭 ARollCut",
    kind: "warning",
    okLabel: "关闭",
    cancelLabel: "取消",
  });
  if (confirmed) {
    await destroyWindow();
  }
}

export function shouldProtectAppClose({
  isDownloadingModel,
  isProcessing,
  isExporting,
  isEditing,
  exportCompleted,
}: {
  isDownloadingModel: boolean;
  isProcessing: boolean;
  isExporting: boolean;
  isEditing: boolean;
  exportCompleted: boolean;
}): boolean {
  return (
    isDownloadingModel ||
    isProcessing ||
    isExporting ||
    (isEditing && !exportCompleted)
  );
}

export function formatMediaTimestamp(
  sample: number,
  sampleRate: number,
): string {
  const totalMilliseconds = Math.floor((sample * 1000) / sampleRate);
  const hours = Math.floor(totalMilliseconds / 3_600_000);
  const minutes = Math.floor((totalMilliseconds % 3_600_000) / 60_000);
  const seconds = Math.floor((totalMilliseconds % 60_000) / 1000);
  const milliseconds = totalMilliseconds % 1000;
  return [hours, minutes, seconds]
    .map((value) => value.toString().padStart(2, "0"))
    .join(":")
    .concat(`,${milliseconds.toString().padStart(3, "0")}`);
}

export function defaultExportName(sourcePath: string): string {
  const fileName = sourcePath.split(/[/\\]/).pop() || "output";
  const separator = fileName.lastIndexOf(".");
  if (separator <= 0) {
    return `${fileName}-cut`;
  }
  return `${fileName.slice(0, separator)}-cut${fileName.slice(separator)}`;
}
