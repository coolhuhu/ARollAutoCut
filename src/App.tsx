import { useEffect, useRef, useState } from "react";

import { APP_NAME, supportedFormatsLabel } from "./app-info";
import {
  cancelExport,
  cancelTranscription,
  chooseExportDestination,
  chooseMediaFile,
  exportEditedMedia,
  formatMediaTimestamp,
  getAudioExportExtension,
  listenForAppClose,
  listenForMediaDrop,
  localMediaUrl,
  shouldProtectAppClose,
  transcribeMedia,
  type ExportProgress,
  type ExportFileKind,
  type ExportMode,
  type ExportResult,
  type TranscriptSegment,
  type TranscriptionProgress,
  type TranscriptionResult,
} from "./media-api";
import {
  cancelModelDownload,
  chooseModelDirectory,
  downloadModel,
  formatDownloadBytes,
  getModelStatus,
  getVadSettings,
  resetVadSettings,
  saveVadSettings,
  type ModelDownloadProgress,
  type ModelStatus,
  type VadSettings,
} from "./model-api";

function UploadIcon() {
  return (
    <svg
      aria-hidden="true"
      className="upload-icon"
      viewBox="0 0 24 24"
      fill="none"
    >
      <path d="M12 16V4m0 0L7.5 8.5M12 4l4.5 4.5" />
      <path d="M5 14v4a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2v-4" />
    </svg>
  );
}

function VideoIcon() {
  return (
    <svg aria-hidden="true" viewBox="0 0 24 24" fill="none">
      <rect x="3" y="6" width="13" height="12" rx="2" />
      <path d="m16 10 5-3v10l-5-3" />
    </svg>
  );
}

function AudioIcon() {
  return (
    <svg aria-hidden="true" viewBox="0 0 24 24" fill="none">
      <path d="M9 18V6l10-2v12" />
      <circle cx="6" cy="18" r="3" />
      <circle cx="16" cy="16" r="3" />
    </svg>
  );
}

const INITIAL_PROGRESS: TranscriptionProgress = {
  stage: "preparing",
  percent: 0,
  message: "正在准备识别任务",
};

type VadSettingsFields = Record<keyof VadSettings, string>;

type PlaybackRange = {
  segmentId: number;
  startSeconds: number;
  endSeconds: number;
  durationSeconds: number;
};

type PlaybackState = {
  segmentId: number | null;
  currentSeconds: number;
  isPlaying: boolean;
};

const DEFAULT_VAD_FIELDS: VadSettingsFields = {
  minSilenceDuration: "0.5",
  minSpeechDuration: "0.25",
  maxSpeechDuration: "20",
};

function exportFileLabel(kind: ExportFileKind): string {
  switch (kind) {
    case "video":
      return "视频文件";
    case "audio":
      return "音频文件";
    case "subtitle":
      return "字幕文件";
  }
}

function vadSettingsToFields(settings: VadSettings): VadSettingsFields {
  return {
    minSilenceDuration: String(settings.minSilenceDuration),
    minSpeechDuration: String(settings.minSpeechDuration),
    maxSpeechDuration: String(settings.maxSpeechDuration),
  };
}

function parseVadSettings(fields: VadSettingsFields): VadSettings {
  const settings = {
    minSilenceDuration: parseVadNumber(fields.minSilenceDuration),
    minSpeechDuration: parseVadNumber(fields.minSpeechDuration),
    maxSpeechDuration: parseVadNumber(fields.maxSpeechDuration),
  };
  validateVadRange(
    "最短静音时长 min_silence_duration",
    settings.minSilenceDuration,
    0.1,
    5,
  );
  validateVadRange(
    "最短语音时长 min_speech_duration",
    settings.minSpeechDuration,
    0.05,
    5,
  );
  validateVadRange(
    "最长语音时长 max_speech_duration",
    settings.maxSpeechDuration,
    5,
    120,
  );
  if (settings.maxSpeechDuration <= settings.minSpeechDuration) {
    throw new Error(
      "最长语音时长 max_speech_duration 必须大于最短语音时长 min_speech_duration",
    );
  }
  return settings;
}

function parseVadNumber(value: string): number {
  const parsed = Number(value);
  if (!Number.isFinite(parsed)) {
    throw new Error("VAD 设置必须填写有效数字");
  }
  return parsed;
}

function validateVadRange(
  label: string,
  value: number,
  minimum: number,
  maximum: number,
) {
  if (value < minimum || value > maximum) {
    throw new Error(`${label} 必须在 ${minimum} 到 ${maximum} 秒之间`);
  }
}

function segmentStartSeconds(segment: TranscriptSegment): number {
  return segment.startSample / segment.sampleRate;
}

function segmentEndSeconds(segment: TranscriptSegment): number {
  return segment.endSample / segment.sampleRate;
}

function segmentDurationSeconds(segment: TranscriptSegment): number {
  return segmentEndSeconds(segment) - segmentStartSeconds(segment);
}

function formatPlaybackClock(seconds: number): string {
  const safeSeconds = Math.max(0, Math.floor(seconds));
  const minutes = Math.floor(safeSeconds / 60);
  const remainingSeconds = safeSeconds % 60;
  return `${minutes.toString().padStart(2, "0")}:${remainingSeconds
    .toString()
    .padStart(2, "0")}`;
}

function clamp(value: number, minimum: number, maximum: number): number {
  return Math.min(maximum, Math.max(minimum, value));
}

export default function App() {
  const [modelStatus, setModelStatus] = useState<ModelStatus | null>(null);
  const [modelError, setModelError] = useState("");
  const [showModelSettings, setShowModelSettings] = useState(false);
  const [phase, setPhase] = useState<"home" | "processing" | "editor">(
    "home",
  );
  const [progress, setProgress] =
    useState<TranscriptionProgress>(INITIAL_PROGRESS);
  const [result, setResult] = useState<TranscriptionResult | null>(null);
  const [segments, setSegments] = useState<TranscriptSegment[]>([]);
  const [operationError, setOperationError] = useState("");
  const [editingId, setEditingId] = useState<number | null>(null);
  const [editingText, setEditingText] = useState("");
  const [editingError, setEditingError] = useState("");
  const [exportProgress, setExportProgress] = useState<ExportProgress | null>(
    null,
  );
  const [exportResult, setExportResult] = useState<ExportResult | null>(null);
  const [showExportMenu, setShowExportMenu] = useState(false);
  const [modelDownloadProgress, setModelDownloadProgress] =
    useState<ModelDownloadProgress | null>(null);
  const [vadFields, setVadFields] =
    useState<VadSettingsFields>(DEFAULT_VAD_FIELDS);
  const [vadError, setVadError] = useState("");
  const [vadMessage, setVadMessage] = useState("");
  const [vadLoading, setVadLoading] = useState(false);
  const closeProtectionRef = useRef(false);
  const exportMenuRef = useRef<HTMLDivElement>(null);
  const previewAudioRef = useRef<HTMLAudioElement>(null);
  const playbackRangeRef = useRef<PlaybackRange | null>(null);
  const [playback, setPlayback] = useState<PlaybackState>({
    segmentId: null,
    currentSeconds: 0,
    isPlaying: false,
  });
  const [playbackError, setPlaybackError] = useState("");
  closeProtectionRef.current = shouldProtectAppClose({
    isDownloadingModel: modelDownloadProgress !== null,
    isProcessing: phase === "processing",
    isExporting: exportProgress !== null,
    isEditing: phase === "editor",
  });

  useEffect(() => {
    void refreshModelStatus();
  }, []);

  useEffect(() => {
    if (showModelSettings) {
      void refreshVadSettings();
    }
  }, [showModelSettings]);

  useEffect(() => {
    if (!showExportMenu) {
      return;
    }

    function closeOnOutsideClick(event: MouseEvent) {
      if (
        event.target instanceof Node &&
        !exportMenuRef.current?.contains(event.target)
      ) {
        setShowExportMenu(false);
      }
    }

    function closeOnEscape(event: KeyboardEvent) {
      if (event.key === "Escape") {
        setShowExportMenu(false);
      }
    }

    document.addEventListener("mousedown", closeOnOutsideClick);
    document.addEventListener("keydown", closeOnEscape);
    return () => {
      document.removeEventListener("mousedown", closeOnOutsideClick);
      document.removeEventListener("keydown", closeOnEscape);
    };
  }, [showExportMenu]);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;

    if (phase === "home" && modelStatus?.state === "ready") {
      void listenForMediaDrop((path) => void startTranscription(path)).then(
        (stopListening) => {
          if (disposed) {
            stopListening();
          } else {
            unlisten = stopListening;
          }
        },
      );
    }

    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [modelStatus?.state, phase]);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;

    void listenForAppClose(() => closeProtectionRef.current).then(
      (stopListening) => {
        if (disposed) {
          stopListening();
        } else {
          unlisten = stopListening;
        }
      },
    );

    return () => {
      if (disposed) {
        return;
      }
      disposed = true;
      unlisten?.();
    };
  }, []);

  async function refreshModelStatus() {
    try {
      setModelError("");
      setModelStatus(await getModelStatus());
    } catch (error) {
      setModelError(String(error));
    }
  }

  async function refreshVadSettings() {
    try {
      setVadLoading(true);
      setVadError("");
      setVadMessage("");
      setVadFields(vadSettingsToFields(await getVadSettings()));
    } catch (error) {
      setVadError(String(error));
    } finally {
      setVadLoading(false);
    }
  }

  async function selectModelDirectory() {
    try {
      setModelError("");
      const status = await chooseModelDirectory();
      if (status) {
        setModelStatus(status);
      }
    } catch (error) {
      setModelError(String(error));
    }
  }

  async function startModelDownload() {
    try {
      setModelError("");
      setModelDownloadProgress({
        stage: "downloading",
        downloadedBytes: 0,
        totalBytes: null,
        percent: 0,
        message: "正在准备下载 SenseVoice 模型",
      });
      setModelStatus(await downloadModel(setModelDownloadProgress));
    } catch (error) {
      const message = String(error);
      if (!message.includes("已取消")) {
        setModelError(message);
      }
    } finally {
      setModelDownloadProgress(null);
    }
  }

  async function stopModelDownload() {
    setModelDownloadProgress((current) =>
      current ? { ...current, message: "正在取消模型下载" } : current,
    );
    await cancelModelDownload();
  }

  function updateVadField(field: keyof VadSettings, value: string) {
    setVadFields((current) => ({ ...current, [field]: value }));
    setVadError("");
    setVadMessage("");
  }

  async function persistVadSettings() {
    try {
      setVadLoading(true);
      setVadError("");
      setVadMessage("");
      const settings = parseVadSettings(vadFields);
      setVadFields(vadSettingsToFields(await saveVadSettings(settings)));
      setVadMessage("VAD 设置已保存，将在下一次识别时生效。");
    } catch (error) {
      setVadError(String(error));
    } finally {
      setVadLoading(false);
    }
  }

  async function restoreDefaultVadSettings() {
    try {
      setVadLoading(true);
      setVadError("");
      setVadMessage("");
      setVadFields(vadSettingsToFields(await resetVadSettings()));
      setVadMessage("已恢复默认 VAD 设置，将在下一次识别时生效。");
    } catch (error) {
      setVadError(String(error));
    } finally {
      setVadLoading(false);
    }
  }

  async function selectMedia() {
    const path = await chooseMediaFile();
    if (path) {
      await startTranscription(path);
    }
  }

  async function startTranscription(path: string) {
    stopPreviewPlayback();
    setOperationError("");
    setPlaybackError("");
    setProgress(INITIAL_PROGRESS);
    setPhase("processing");
    try {
      const transcription = await transcribeMedia(path, setProgress);
      setResult(transcription);
      setSegments(transcription.segments);
      setPhase("editor");
    } catch (error) {
      const message = String(error);
      if (!message.includes("已取消")) {
        setOperationError(message);
      }
      setPhase("home");
    }
  }

  async function stopTranscription() {
    setProgress((current) => ({
      ...current,
      message: "正在取消识别任务",
    }));
    await cancelTranscription();
  }

  async function reupload() {
    if (
      phase === "editor" &&
      !window.confirm("重新上传将放弃当前字幕编辑，是否继续？")
    ) {
      return;
    }
    const path = await chooseMediaFile();
    if (!path) {
      return;
    }
    setExportResult(null);
    setEditingId(null);
    setEditingText("");
    setEditingError("");
    setShowExportMenu(false);
    stopPreviewPlayback();
    await startTranscription(path);
  }

  function beginEditing(segment: TranscriptSegment) {
    setEditingId(segment.id);
    setEditingText(segment.editedText);
    setEditingError("");
  }

  function saveEditing() {
    const text = editingText.trim();
    if (editingId === null || text.length === 0) {
      return;
    }
    if (/[\r\n]/u.test(text)) {
      setEditingError("字幕内容不能换行编辑，请删除换行后再保存。");
      return;
    }
    setSegments((current) =>
      current.map((segment) =>
        segment.id === editingId ? { ...segment, editedText: text } : segment,
      ),
    );
    setEditingId(null);
    setEditingText("");
    setEditingError("");
  }

  function cancelEditing() {
    setEditingId(null);
    setEditingText("");
    setEditingError("");
  }

  function resizeEditingArea(element: HTMLTextAreaElement) {
    element.style.height = "auto";
    element.style.height = `${element.scrollHeight}px`;
  }

  function setRetained(id: number, retained: boolean) {
    setSegments((current) =>
      current.map((segment) =>
        segment.id === id ? { ...segment, retained } : segment,
      ),
    );
    if (!retained && editingId === id) {
      setEditingId(null);
      setEditingText("");
      setEditingError("");
    }
  }

  function stopPreviewPlayback() {
    const audio = previewAudioRef.current;
    if (audio) {
      audio.pause();
    }
    playbackRangeRef.current = null;
    setPlayback({ segmentId: null, currentSeconds: 0, isPlaying: false });
  }

  async function toggleSegmentPlayback(segment: TranscriptSegment) {
    const audio = previewAudioRef.current;
    if (!audio) {
      setPlaybackError("无法播放该媒体片段，请确认原文件仍存在且格式可播放。");
      return;
    }
    const durationSeconds = segmentDurationSeconds(segment);
    const range = {
      segmentId: segment.id,
      startSeconds: segmentStartSeconds(segment),
      endSeconds: segmentEndSeconds(segment),
      durationSeconds,
    };

    if (playback.segmentId === segment.id && playback.isPlaying) {
      audio.pause();
      setPlayback((current) => ({ ...current, isPlaying: false }));
      return;
    }

    playbackRangeRef.current = range;
    const resumeSeconds =
      playback.segmentId === segment.id && playback.currentSeconds < durationSeconds
        ? playback.currentSeconds
        : 0;
    audio.currentTime = range.startSeconds + resumeSeconds;
    setPlayback({
      segmentId: segment.id,
      currentSeconds: resumeSeconds,
      isPlaying: true,
    });
    setPlaybackError("");

    try {
      await audio.play();
    } catch {
      setPlayback({
        segmentId: segment.id,
        currentSeconds: resumeSeconds,
        isPlaying: false,
      });
      setPlaybackError("无法播放该媒体片段，请确认原文件仍存在且格式可播放。");
    }
  }

  function seekSegmentPlayback(segment: TranscriptSegment, value: string) {
    const audio = previewAudioRef.current;
    const durationSeconds = segmentDurationSeconds(segment);
    const currentSeconds = clamp(Number(value), 0, durationSeconds);
    const range = {
      segmentId: segment.id,
      startSeconds: segmentStartSeconds(segment),
      endSeconds: segmentEndSeconds(segment),
      durationSeconds,
    };
    playbackRangeRef.current = range;
    if (audio) {
      audio.currentTime = range.startSeconds + currentSeconds;
    }
    setPlayback((current) => ({
      segmentId: segment.id,
      currentSeconds,
      isPlaying: current.segmentId === segment.id && current.isPlaying,
    }));
  }

  function updatePreviewPlaybackProgress() {
    const audio = previewAudioRef.current;
    const range = playbackRangeRef.current;
    if (!audio || !range) {
      return;
    }
    const currentSeconds = clamp(
      audio.currentTime - range.startSeconds,
      0,
      range.durationSeconds,
    );
    if (audio.currentTime >= range.endSeconds) {
      audio.pause();
      audio.currentTime = range.endSeconds;
      setPlayback({
        segmentId: range.segmentId,
        currentSeconds: range.durationSeconds,
        isPlaying: false,
      });
      return;
    }
    setPlayback({
      segmentId: range.segmentId,
      currentSeconds,
      isPlaying: !audio.paused,
    });
  }

  function finishPreviewPlayback() {
    setPlayback((current) => ({ ...current, isPlaying: false }));
  }

  async function startExport(mode: ExportMode) {
    if (!result) {
      return;
    }

    setShowExportMenu(false);
    setOperationError("");
    try {
      const audioExtension =
        result.mediaKind === "video" &&
        (mode === "audioWithSubtitle" || mode === "audioOnly")
          ? await getAudioExportExtension(result.sourcePath)
          : undefined;
      const destination = await chooseExportDestination(
        result.sourcePath,
        mode,
        audioExtension,
      );
      if (!destination) {
        return;
      }

      setExportResult(null);
      setExportProgress({ percent: 0, message: "正在准备导出" });
      setExportResult(
        await exportEditedMedia(
          result.sourcePath,
          destination,
          result.mediaKind,
          mode,
          segments,
          setExportProgress,
        ),
      );
    } catch (error) {
      const message = String(error);
      if (!message.includes("已取消")) {
        setOperationError(message);
      }
    } finally {
      setExportProgress(null);
    }
  }

  async function stopExport() {
    setExportProgress((current) =>
      current ? { ...current, message: "正在取消导出" } : current,
    );
    await cancelExport();
  }

  function continueEditing() {
    setExportResult(null);
  }

  const modelReady = modelStatus?.state === "ready";
  const statusTitle = modelReady
    ? "SenseVoice 模型可用"
    : modelStatus?.state === "invalid"
      ? "SenseVoice 模型校验失败"
      : "SenseVoice 模型尚未配置";

  if (phase === "processing") {
    return (
      <main className="processing-shell">
        <section className="processing-card" aria-live="polite">
          <span className="processing-mark">ASR</span>
          <h1>正在生成字幕</h1>
          <p>{progress.message}</p>
          <div
            className="progress-track"
            role="progressbar"
            aria-label="识别进度"
            aria-valuemin={0}
            aria-valuemax={100}
            aria-valuenow={progress.percent}
          >
            <span style={{ width: `${progress.percent}%` }} />
          </div>
          <strong>{progress.percent}%</strong>
          <button
            className="secondary-button"
            type="button"
            onClick={() => void stopTranscription()}
          >
            取消识别
          </button>
        </section>
      </main>
    );
  }

  if (phase === "editor" && result) {
    const retainedCount = segments.filter((segment) => segment.retained).length;
    const defaultExportMode: ExportMode =
      result.mediaKind === "video"
        ? "videoWithSubtitle"
        : "audioWithSubtitle";
    const previewAudioUrl = localMediaUrl(result.previewAudioPath);
    return (
      <main className="editor-shell">
        <audio
          className="preview-audio"
          data-testid="preview-audio"
          ref={previewAudioRef}
          src={previewAudioUrl}
          preload="auto"
          onEnded={finishPreviewPlayback}
          onError={() =>
            setPlaybackError("无法播放该媒体片段，请确认原文件仍存在且格式可播放。")
          }
          onTimeUpdate={updatePreviewPlaybackProgress}
        />
        <header className="app-header">
          <div>
            <strong>{APP_NAME}</strong>
            <span>{result.sourceName}</span>
          </div>
          <div className="header-actions">
            <button
              className="secondary-button"
              type="button"
              onClick={() => void reupload()}
            >
              重新上传
            </button>
            <div className="export-menu" ref={exportMenuRef}>
              <div className="split-export-button">
                <button
                  className="primary-button export-main-button"
                  type="button"
                  disabled={retainedCount === 0}
                  onClick={() => void startExport(defaultExportMode)}
                >
                  导出{result.mediaKind === "video" ? "视频" : "音频"}
                </button>
                <button
                  className="primary-button export-menu-button"
                  type="button"
                  aria-label="更多导出选项"
                  aria-haspopup="menu"
                  aria-expanded={showExportMenu}
                  disabled={retainedCount === 0}
                  onClick={() => setShowExportMenu((visible) => !visible)}
                >
                  <span aria-hidden="true">⌄</span>
                </button>
              </div>
              {showExportMenu && (
                <div className="export-options" role="menu">
                  {result.mediaKind === "video" ? (
                    <>
                      <button
                        type="button"
                        role="menuitem"
                        onClick={() => void startExport("audioWithSubtitle")}
                      >
                        导出音频和字幕
                      </button>
                      <button
                        type="button"
                        role="menuitem"
                        onClick={() => void startExport("audioOnly")}
                      >
                        仅导出音频
                      </button>
                    </>
                  ) : (
                    <button
                      type="button"
                      role="menuitem"
                      onClick={() => void startExport("subtitleOnly")}
                    >
                      仅导出字幕
                    </button>
                  )}
                </div>
              )}
            </div>
          </div>
        </header>

        <section className="editor-content" aria-labelledby="editor-title">
          <div className="editor-intro">
            <div>
              <p className="eyebrow">字幕编辑</p>
              <h1 id="editor-title">保留需要的口播内容</h1>
              <p>修正识别文字，或删除不需要的字幕片段。</p>
            </div>
            <div className="segment-summary">
              已保留 <strong>{retainedCount}</strong> / {segments.length} 段
            </div>
          </div>
          {operationError && (
            <p className="operation-error editor-error" role="alert">
              {operationError}
            </p>
          )}
          {playbackError && (
            <p className="operation-error editor-error" role="alert">
              {playbackError}
            </p>
          )}

          <div className="subtitle-list">
            {segments.map((segment) => {
              const duration = segmentDurationSeconds(segment);
              const isCurrentPlayback = playback.segmentId === segment.id;
              const playbackSeconds = isCurrentPlayback
                ? clamp(playback.currentSeconds, 0, duration)
                : 0;
              const isSegmentPlaying =
                isCurrentPlayback && playback.isPlaying;
              const isEditing = editingId === segment.id;
              return (
                <article
                  className={`subtitle-card ${segment.retained ? "" : "subtitle-deleted"}`}
                  key={segment.id}
                >
                  <span className="segment-index">{segment.id}</span>
                  <div className="segment-body">
                    <div className="segment-time">
                      <code>
                        {formatMediaTimestamp(
                          segment.startSample,
                          segment.sampleRate,
                        )}{" "}
                        →{" "}
                        {formatMediaTimestamp(
                          segment.endSample,
                          segment.sampleRate,
                        )}
                      </code>
                      <span className="segment-duration">
                        {duration.toFixed(1)}s
                      </span>
                      <div className="segment-player">
                        <button
                          className="segment-play-button"
                          type="button"
                          aria-label={`${isSegmentPlaying ? "暂停" : "播放"}第 ${segment.id} 条字幕`}
                          onClick={() => void toggleSegmentPlayback(segment)}
                        >
                          {isSegmentPlaying ? "暂停" : "播放"}
                        </button>
                        <span className="playback-time">
                          {formatPlaybackClock(playbackSeconds)}
                        </span>
                        <input
                          className="segment-playback-range"
                          type="range"
                          min={0}
                          max={duration}
                          step={0.05}
                          value={playbackSeconds}
                          aria-label={`第 ${segment.id} 条字幕播放进度`}
                          onChange={(event) =>
                            seekSegmentPlayback(
                              segment,
                              event.currentTarget.value,
                            )
                          }
                        />
                        <span className="playback-time">
                          {formatPlaybackClock(duration)}
                        </span>
                      </div>
                    </div>
                    {isEditing ? (
                      <div className="edit-row">
                        <textarea
                          className="segment-text-input"
                          aria-label={`编辑第 ${segment.id} 条字幕`}
                          rows={1}
                          ref={(element) => {
                            if (element) {
                              resizeEditingArea(element);
                            }
                          }}
                          value={editingText}
                          onChange={(event) => {
                            const value = event.currentTarget.value;
                            setEditingText(value);
                            if (!/[\r\n]/u.test(value)) {
                              setEditingError("");
                            }
                            resizeEditingArea(event.currentTarget);
                          }}
                          onKeyDown={(event) => {
                            if (event.key === "Escape") {
                              cancelEditing();
                            }
                          }}
                          autoFocus
                        />
                        {editingError && (
                          <p className="edit-error" role="alert">
                            {editingError}
                          </p>
                        )}
                      </div>
                    ) : (
                      <p>{segment.editedText}</p>
                    )}
                  </div>
                  <div className="segment-actions">
                    {isEditing ? (
                      <>
                        <button
                          type="button"
                          onClick={saveEditing}
                          disabled={editingText.trim().length === 0}
                        >
                          保存
                        </button>
                        <button type="button" onClick={cancelEditing}>
                          取消
                        </button>
                      </>
                    ) : segment.retained ? (
                      <>
                        <button
                          type="button"
                          onClick={() => beginEditing(segment)}
                        >
                          修正
                        </button>
                        <button
                          className="danger-button"
                          type="button"
                          onClick={() => setRetained(segment.id, false)}
                        >
                          删除
                        </button>
                      </>
                    ) : (
                      <button
                        type="button"
                        onClick={() => setRetained(segment.id, true)}
                      >
                        恢复
                      </button>
                    )}
                  </div>
                </article>
              );
            })}
          </div>
        </section>

        {exportProgress && (
          <div className="modal-backdrop" role="presentation">
            <section
              className="export-dialog"
              role="dialog"
              aria-modal="true"
              aria-label="正在导出"
            >
              <span className="processing-mark">OUT</span>
              <h2>正在导出文件</h2>
              <p>{exportProgress.message}</p>
              <div
                className="progress-track"
                role="progressbar"
                aria-label="导出进度"
                aria-valuemin={0}
                aria-valuemax={100}
                aria-valuenow={exportProgress.percent}
              >
                <span style={{ width: `${exportProgress.percent}%` }} />
              </div>
              <strong>{exportProgress.percent}%</strong>
              <button
                className="secondary-button"
                type="button"
                onClick={() => void stopExport()}
              >
                取消导出
              </button>
            </section>
          </div>
        )}

        {exportResult && (
          <div className="modal-backdrop" role="presentation">
            <section
              className="export-complete-dialog"
              role="dialog"
              aria-modal="true"
              aria-labelledby="export-complete-title"
            >
              <span className="success-mark">完成</span>
              <h2 id="export-complete-title">导出完成</h2>
              <div className="exported-files">
                {exportResult.files.map((file) => (
                  <div key={`${file.kind}-${file.path}`}>
                    <p>{exportFileLabel(file.kind)}</p>
                    <code>{file.path}</code>
                  </div>
                ))}
              </div>
              <button
                className="primary-button"
                type="button"
                onClick={continueEditing}
              >
                继续编辑
              </button>
            </section>
          </div>
        )}
      </main>
    );
  }

  return (
    <main className="home-shell">
      <section className="hero" aria-labelledby="page-title">
        <p className="eyebrow">{APP_NAME}</p>
        <h1 id="page-title">视频字幕编辑器</h1>
        <p className="subtitle">
          上传视频或音频文件，自动生成字幕，通过编辑字幕来剪辑内容
        </p>

        <button
          className="upload-card"
          type="button"
          aria-describedby="format-detail"
          disabled={!modelReady}
          onClick={() => void selectMedia()}
        >
          <span className="upload-icon-wrap">
            <UploadIcon />
          </span>
          <strong>点击上传或拖拽文件到此处</strong>
          <span>支持常见音视频格式</span>
          <span className="media-kinds">
            <span>
              <VideoIcon />
              视频
            </span>
            <span>
              <AudioIcon />
              音频
            </span>
          </span>
        </button>

        <p id="format-detail" className="format-detail">
          支持格式：{supportedFormatsLabel()}
        </p>
        {operationError && (
          <p className="operation-error" role="alert">
            {operationError}
          </p>
        )}

        <aside
          className={`model-notice ${modelReady ? "model-notice-ready" : ""}`}
          aria-label="模型状态"
        >
          <span className="status-dot" />
          <div>
            <strong>{statusTitle}</strong>
            <p>
              {modelReady
                ? "现在可以导入媒体文件。"
                : "完成模型设置后即可导入媒体文件。"}
            </p>
          </div>
          <button type="button" onClick={() => setShowModelSettings(true)}>
            模型设置
          </button>
        </aside>

        <section className="workflow" aria-labelledby="workflow-title">
          <h2 id="workflow-title">工作流程</h2>
          <ol>
            <li>上传视频或音频文件</li>
            <li>系统自动执行语音检测和识别</li>
            <li>修正字幕文字，删除不需要的片段</li>
            <li>系统自动提取保留内容并重新拼接</li>
            <li>导出最终媒体和字幕文件</li>
          </ol>
        </section>
      </section>

      {showModelSettings && (
        <div className="modal-backdrop" role="presentation">
          <section
            className="model-dialog"
            role="dialog"
            aria-modal="true"
            aria-labelledby="model-dialog-title"
          >
            <div className="dialog-header">
              <div>
                <p className="eyebrow">SenseVoice</p>
                <h2 id="model-dialog-title">模型设置</h2>
              </div>
              <button
                className="icon-button"
                type="button"
                aria-label="关闭模型设置"
                disabled={modelDownloadProgress !== null}
                onClick={() => setShowModelSettings(false)}
              >
                ×
              </button>
            </div>

            <section className="settings-section" aria-labelledby="sensevoice-settings-title">
              <h3 id="sensevoice-settings-title">SenseVoice 模型</h3>
              <div className="model-status-row">
                <span
                  className={`status-badge status-${modelStatus?.state ?? "checking"}`}
                >
                  {modelStatus === null
                    ? "检查中"
                    : modelReady
                      ? "可用"
                      : modelStatus.state === "invalid"
                        ? "校验失败"
                        : "缺失"}
                </span>
                <code>{modelStatus?.directory ?? "正在读取模型目录..."}</code>
              </div>

              {(modelStatus?.issues.length ?? 0) > 0 && (
                <ul className="model-issues">
                  {modelStatus?.issues.map((issue) => (
                    <li key={issue}>{issue}</li>
                  ))}
                </ul>
              )}
              {modelError && <p className="dialog-error">{modelError}</p>}
            </section>

            {modelDownloadProgress ? (
              <div className="model-download-panel" aria-live="polite">
                <div className="download-heading">
                  <strong>{modelDownloadProgress.message}</strong>
                  <span>{modelDownloadProgress.percent}%</span>
                </div>
                <div
                  className="progress-track"
                  role="progressbar"
                  aria-label="模型下载进度"
                  aria-valuemin={0}
                  aria-valuemax={100}
                  aria-valuenow={modelDownloadProgress.percent}
                >
                  <span
                    style={{ width: `${modelDownloadProgress.percent}%` }}
                  />
                </div>
                {modelDownloadProgress.stage === "downloading" && (
                  <p>
                    {formatDownloadBytes(
                      modelDownloadProgress.downloadedBytes,
                      modelDownloadProgress.totalBytes,
                    )}
                  </p>
                )}
                <button
                  className="secondary-button"
                  type="button"
                  onClick={() => void stopModelDownload()}
                >
                  取消下载
                </button>
              </div>
            ) : (
              <div className="dialog-actions model-actions">
                <button
                  className="secondary-button"
                  type="button"
                  onClick={() => void selectModelDirectory()}
                >
                  选择已有模型目录
                </button>
                <button
                  className="primary-button"
                  type="button"
                  onClick={() => void startModelDownload()}
                >
                  {modelError ? "重试下载模型" : "下载模型"}
                </button>
              </div>
            )}

            <section className="settings-section" aria-labelledby="vad-settings-title">
              <div>
                <h3 id="vad-settings-title">VAD 设置</h3>
                <p>
                  设置会在下一次上传识别时生效；正在识别的任务不会动态变更。
                </p>
              </div>
              <div className="vad-settings-grid">
                <label>
                  <span>最短静音时长 min_silence_duration</span>
                  <input
                    type="number"
                    aria-label="最短静音时长 min_silence_duration"
                    min="0.1"
                    max="5"
                    step="0.05"
                    value={vadFields.minSilenceDuration}
                    disabled={vadLoading || modelDownloadProgress !== null}
                    onChange={(event) =>
                      updateVadField(
                        "minSilenceDuration",
                        event.currentTarget.value,
                      )
                    }
                  />
                  <small>0.1 到 5.0 秒，默认 0.5 秒</small>
                </label>
                <label>
                  <span>最短语音时长 min_speech_duration</span>
                  <input
                    type="number"
                    aria-label="最短语音时长 min_speech_duration"
                    min="0.05"
                    max="5"
                    step="0.05"
                    value={vadFields.minSpeechDuration}
                    disabled={vadLoading || modelDownloadProgress !== null}
                    onChange={(event) =>
                      updateVadField(
                        "minSpeechDuration",
                        event.currentTarget.value,
                      )
                    }
                  />
                  <small>0.05 到 5.0 秒，默认 0.25 秒</small>
                </label>
                <label>
                  <span>最长语音时长 max_speech_duration</span>
                  <input
                    type="number"
                    aria-label="最长语音时长 max_speech_duration"
                    min="5"
                    max="120"
                    step="1"
                    value={vadFields.maxSpeechDuration}
                    disabled={vadLoading || modelDownloadProgress !== null}
                    onChange={(event) =>
                      updateVadField(
                        "maxSpeechDuration",
                        event.currentTarget.value,
                      )
                    }
                  />
                  <small>5.0 到 120.0 秒，默认 20.0 秒</small>
                </label>
              </div>
              {vadError && <p className="dialog-error">{vadError}</p>}
              {vadMessage && <p className="dialog-success">{vadMessage}</p>}
              <div className="dialog-actions">
                <button
                  className="secondary-button"
                  type="button"
                  disabled={vadLoading || modelDownloadProgress !== null}
                  onClick={() => void restoreDefaultVadSettings()}
                >
                  恢复默认值
                </button>
                <button
                  className="primary-button"
                  type="button"
                  disabled={vadLoading || modelDownloadProgress !== null}
                  onClick={() => void persistVadSettings()}
                >
                  保存 VAD 设置
                </button>
              </div>
            </section>
          </section>
        </div>
      )}
    </main>
  );
}
