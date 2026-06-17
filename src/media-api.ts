import { convertFileSrc, invoke } from "@tauri-apps/api/core";
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
  previewAudioPath: string;
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

export type ExportMode =
  | "videoWithSubtitle"
  | "audioWithSubtitle"
  | "audioOnly"
  | "subtitleOnly";

export type ExportFileKind = "video" | "audio" | "subtitle";

export interface ExportedFile {
  kind: ExportFileKind;
  path: string;
}

export interface ExportResult {
  files: ExportedFile[];
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
  mode: ExportMode,
  audioExtension?: string,
): Promise<string | null> {
  if (!isTauriRuntime()) {
    return null;
  }
  const extension = exportExtension(sourcePath, mode, audioExtension);
  const title =
    mode === "subtitleOnly"
      ? "导出字幕文件"
      : mode === "videoWithSubtitle"
        ? "导出剪辑后的视频"
        : "导出剪辑后的音频";
  return save({
    title,
    defaultPath: defaultExportName(sourcePath, mode, audioExtension),
    filters: extension
      ? [{ name: `${extension.toUpperCase()} 文件`, extensions: [extension] }]
      : undefined,
  });
}

export async function exportEditedMedia(
  sourcePath: string,
  destinationPath: string,
  mediaKind: "audio" | "video",
  mode: ExportMode,
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
      mode,
      segments,
    });
  } finally {
    unlisten();
  }
}

export async function getAudioExportExtension(
  sourcePath: string,
): Promise<string> {
  if (!isTauriRuntime()) {
    throw new Error("请在 Tauri App 中读取音频编码信息");
  }
  return invoke<string>("get_audio_export_extension", { sourcePath });
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
}: {
  isDownloadingModel: boolean;
  isProcessing: boolean;
  isExporting: boolean;
  isEditing: boolean;
}): boolean {
  return (
    isDownloadingModel ||
    isProcessing ||
    isExporting ||
    isEditing
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

export function localMediaUrl(path: string): string {
  return isTauriRuntime() ? convertFileSrc(path) : path;
}

export function defaultExportName(
  sourcePath: string,
  mode: ExportMode,
  audioExtension?: string,
): string {
  const fileName = sourcePath.split(/[/\\]/).pop() || "output";
  const separator = fileName.lastIndexOf(".");
  const stem = separator <= 0 ? fileName : fileName.slice(0, separator);
  const sourceExtension = separator <= 0 ? "" : fileName.slice(separator + 1);

  if (mode === "subtitleOnly") {
    return `${stem}-cut.srt`;
  }
  if (
    (mode === "audioWithSubtitle" || mode === "audioOnly") &&
    audioExtension
  ) {
    return `${stem}-cut-audio.${audioExtension}`;
  }
  return sourceExtension
    ? `${stem}-cut.${sourceExtension}`
    : `${stem}-cut`;
}

function exportExtension(
  sourcePath: string,
  mode: ExportMode,
  audioExtension?: string,
): string {
  if (mode === "subtitleOnly") {
    return "srt";
  }
  if (
    (mode === "audioWithSubtitle" || mode === "audioOnly") &&
    audioExtension
  ) {
    return audioExtension.toLowerCase();
  }
  const fileName = sourcePath.split(/[/\\]/).pop() ?? "";
  const separator = fileName.lastIndexOf(".");
  return separator <= 0 ? "" : fileName.slice(separator + 1).toLowerCase();
}
