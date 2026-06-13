import { useEffect, useRef, useState } from "react";

import { APP_NAME, supportedFormatsLabel } from "./app-info";
import {
  cancelExport,
  cancelTranscription,
  chooseExportDestination,
  chooseMediaFile,
  exportEditedMedia,
  formatMediaTimestamp,
  listenForAppClose,
  listenForMediaDrop,
  shouldProtectAppClose,
  transcribeMedia,
  type ExportProgress,
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
  type ModelDownloadProgress,
  type ModelStatus,
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
  const [exportProgress, setExportProgress] = useState<ExportProgress | null>(
    null,
  );
  const [exportResult, setExportResult] = useState<ExportResult | null>(null);
  const [modelDownloadProgress, setModelDownloadProgress] =
    useState<ModelDownloadProgress | null>(null);
  const closeProtectionRef = useRef(false);
  closeProtectionRef.current = shouldProtectAppClose({
    isDownloadingModel: modelDownloadProgress !== null,
    isProcessing: phase === "processing",
    isExporting: exportProgress !== null,
    isEditing: phase === "editor",
    exportCompleted: exportResult !== null,
  });

  useEffect(() => {
    void refreshModelStatus();
  }, []);

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

  async function selectMedia() {
    const path = await chooseMediaFile();
    if (path) {
      await startTranscription(path);
    }
  }

  async function startTranscription(path: string) {
    setOperationError("");
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
    setResult(null);
    setSegments([]);
    setEditingId(null);
    setPhase("home");
    await selectMedia();
  }

  function beginEditing(segment: TranscriptSegment) {
    setEditingId(segment.id);
    setEditingText(segment.editedText);
  }

  function saveEditing() {
    const text = editingText.trim();
    if (editingId === null || text.length === 0) {
      return;
    }
    setSegments((current) =>
      current.map((segment) =>
        segment.id === editingId ? { ...segment, editedText: text } : segment,
      ),
    );
    setEditingId(null);
    setEditingText("");
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
    }
  }

  async function startExport() {
    if (!result) {
      return;
    }
    const destination = await chooseExportDestination(result.sourcePath);
    if (!destination) {
      return;
    }

    setOperationError("");
    setExportResult(null);
    setExportProgress({ percent: 0, message: "正在准备导出" });
    try {
      setExportResult(
        await exportEditedMedia(
          result.sourcePath,
          destination,
          result.mediaKind,
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

  function finishEditing() {
    setExportResult(null);
    setResult(null);
    setSegments([]);
    setPhase("home");
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
    return (
      <main className="editor-shell">
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
            <button
              className="primary-button"
              type="button"
              disabled={retainedCount === 0}
              onClick={() => void startExport()}
            >
              导出{result.mediaKind === "video" ? "视频" : "音频"}
            </button>
          </div>
        </header>

        <section className="editor-content" aria-labelledby="editor-title">
          <div className="editor-intro">
            <div>
              <p className="eyebrow">字幕编辑</p>
              <h1 id="editor-title">保留需要的口播内容</h1>
              <p>
                修正识别文字，或删除不需要的字幕片段。导出时将严格使用
                VAD 时间边界。
              </p>
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

          <div className="subtitle-list">
            {segments.map((segment) => {
              const duration =
                (segment.endSample - segment.startSample) / segment.sampleRate;
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
                      <span>{duration.toFixed(1)}s</span>
                    </div>
                    {isEditing ? (
                      <div className="edit-row">
                        <input
                          aria-label={`编辑第 ${segment.id} 条字幕`}
                          value={editingText}
                          onChange={(event) =>
                            setEditingText(event.currentTarget.value)
                          }
                          onKeyDown={(event) => {
                            if (event.key === "Enter") {
                              saveEditing();
                            }
                            if (event.key === "Escape") {
                              setEditingId(null);
                            }
                          }}
                          autoFocus
                        />
                        <button
                          type="button"
                          onClick={saveEditing}
                          disabled={editingText.trim().length === 0}
                        >
                          保存
                        </button>
                        <button
                          type="button"
                          onClick={() => setEditingId(null)}
                        >
                          取消
                        </button>
                      </div>
                    ) : (
                      <p>{segment.editedText}</p>
                    )}
                  </div>
                  <div className="segment-actions">
                    {segment.retained ? (
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
              <h2>正在导出媒体</h2>
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
              <p>媒体文件</p>
              <code>{exportResult.mediaPath}</code>
              <p>字幕文件</p>
              <code>{exportResult.subtitlePath}</code>
              <button
                className="primary-button"
                type="button"
                onClick={finishEditing}
              >
                完成本次编辑
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
              <div className="dialog-actions">
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
          </section>
        </div>
      )}
    </main>
  );
}
