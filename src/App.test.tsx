import {
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import App from "./App";
import {
  chooseExportDestination,
  chooseMediaFile,
  exportEditedMedia,
  transcribeMedia,
} from "./media-api";
import { downloadModel, getModelStatus } from "./model-api";

vi.mock("./model-api", () => ({
  getModelStatus: vi.fn(),
  chooseModelDirectory: vi.fn(),
  downloadModel: vi.fn(),
  cancelModelDownload: vi.fn(),
  formatDownloadBytes: (downloadedBytes: number, totalBytes: number | null) =>
    totalBytes === null
      ? `${downloadedBytes} B`
      : `${downloadedBytes} B / ${totalBytes} B`,
}));

vi.mock("./media-api", async (importOriginal) => {
  const original = await importOriginal<typeof import("./media-api")>();
  return {
    ...original,
    chooseMediaFile: vi.fn(),
    transcribeMedia: vi.fn(),
    cancelTranscription: vi.fn(),
    chooseExportDestination: vi.fn(),
    exportEditedMedia: vi.fn(),
    cancelExport: vi.fn(),
    listenForMediaDrop: vi.fn().mockResolvedValue(() => {}),
    listenForAppClose: vi.fn().mockResolvedValue(() => {}),
  };
});

describe("App", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(chooseMediaFile).mockResolvedValue(null);
    vi.mocked(chooseExportDestination).mockResolvedValue(null);
    vi.mocked(getModelStatus).mockResolvedValue({
      state: "missing",
      directory: "/App Data/models/sense-voice",
      issues: ["缺少文件：model.int8.onnx"],
    });
  });

  it("renders the initial upload experience", async () => {
    render(<App />);

    expect(
      screen.getByRole("heading", { name: "视频字幕编辑器" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /点击上传或拖拽文件到此处/ }),
    ).toBeDisabled();
    expect(
      await screen.findByText("SenseVoice 模型尚未配置"),
    ).toBeInTheDocument();
  });

  it("shows model validation details in settings", async () => {
    render(<App />);
    await screen.findByText("SenseVoice 模型尚未配置");

    fireEvent.click(screen.getByRole("button", { name: "模型设置" }));

    expect(
      screen.getByRole("dialog", { name: "模型设置" }),
    ).toBeInTheDocument();
    expect(screen.getByText("/App Data/models/sense-voice")).toBeInTheDocument();
    expect(screen.getByText("缺少文件：model.int8.onnx")).toBeInTheDocument();
  });

  it("enables upload when the model is ready", async () => {
    vi.mocked(getModelStatus).mockResolvedValue({
      state: "ready",
      directory: "/models/sense-voice",
      issues: [],
    });

    render(<App />);

    await waitFor(() =>
      expect(
        screen.getByRole("button", { name: /点击上传或拖拽文件到此处/ }),
      ).toBeEnabled(),
    );
  });

  it("downloads the model and enables media upload", async () => {
    vi.mocked(downloadModel).mockImplementation(async (onProgress) => {
      onProgress({
        stage: "downloading",
        downloadedBytes: 50,
        totalBytes: 100,
        percent: 45,
        message: "正在下载 SenseVoice 模型",
      });
      return {
        state: "ready",
        directory: "/App Data/models/sense-voice",
        issues: [],
      };
    });

    render(<App />);
    await screen.findByText("SenseVoice 模型尚未配置");
    fireEvent.click(screen.getByRole("button", { name: "模型设置" }));
    fireEvent.click(screen.getByRole("button", { name: "下载模型" }));

    expect(
      await screen.findByText("SenseVoice 模型可用"),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /点击上传或拖拽文件到此处/ }),
    ).toBeEnabled();
  });

  it("edits recognized subtitles and exports the current state", async () => {
    vi.mocked(getModelStatus).mockResolvedValue({
      state: "ready",
      directory: "/models/sense-voice",
      issues: [],
    });
    vi.mocked(chooseMediaFile).mockResolvedValue("/tmp/vad-test.wav");
    vi.mocked(transcribeMedia).mockImplementation(async (_path, onProgress) => {
      onProgress({
        stage: "recognizing",
        percent: 70,
        message: "正在识别第 1/2 个语音片段",
      });
      return {
        sourcePath: "/tmp/vad-test.wav",
        sourceName: "vad-test.wav",
        mediaKind: "audio",
        segments: [
          {
            id: 1,
            startSample: 0,
            endSample: 16_000,
            sampleRate: 16_000,
            originalText: "第一句",
            editedText: "第一句",
            retained: true,
          },
          {
            id: 2,
            startSample: 24_000,
            endSample: 40_000,
            sampleRate: 16_000,
            originalText: "第二句",
            editedText: "第二句",
            retained: true,
          },
        ],
      };
    });

    render(<App />);
    const upload = await screen.findByRole("button", {
      name: /点击上传或拖拽文件到此处/,
    });
    await waitFor(() => expect(upload).toBeEnabled());
    fireEvent.click(upload);

    expect(
      await screen.findByRole("heading", { name: "保留需要的口播内容" }),
    ).toBeInTheDocument();
    expect(
      screen.getByText("修正识别文字，或删除不需要的字幕片段。"),
    ).toBeInTheDocument();
    expect(
      screen.queryByText(/导出时将严格使用 VAD 时间边界/),
    ).not.toBeInTheDocument();
    expect(screen.getByText("00:00:01,500 → 00:00:02,500")).toBeInTheDocument();

    fireEvent.click(screen.getAllByRole("button", { name: "删除" })[0]);
    expect(screen.getByText(/已保留/)).toHaveTextContent("1 / 2 段");
    fireEvent.click(screen.getByRole("button", { name: "恢复" }));
    expect(screen.getByText(/已保留/)).toHaveTextContent("2 / 2 段");

    fireEvent.click(screen.getAllByRole("button", { name: "修正" })[0]);
    const editingInput = screen.getByRole("textbox", {
      name: "编辑第 1 条字幕",
    });
    const editingCard = editingInput.closest("article");
    expect(editingCard).not.toBeNull();
    expect(editingInput.tagName).toBe("TEXTAREA");
    expect(editingInput).toHaveClass("segment-text-input");
    expect(
      within(editingCard!).queryByRole("button", { name: "修正" }),
    ).not.toBeInTheDocument();
    expect(
      within(editingCard!).queryByRole("button", { name: "删除" }),
    ).not.toBeInTheDocument();
    expect(
      within(editingCard!).getByRole("button", { name: "保存" }),
    ).toBeInTheDocument();
    expect(
      within(editingCard!).getByRole("button", { name: "取消" }),
    ).toBeInTheDocument();

    fireEvent.change(editingInput, {
      target: { value: "不会保存的草稿" },
    });
    fireEvent.click(
      within(editingCard!).getByRole("button", { name: "取消" }),
    );
    expect(screen.getByText("第一句")).toBeInTheDocument();
    expect(
      within(editingCard!).getByRole("button", { name: "修正" }),
    ).toBeInTheDocument();
    expect(
      within(editingCard!).getByRole("button", { name: "删除" }),
    ).toBeInTheDocument();

    fireEvent.click(
      within(editingCard!).getByRole("button", { name: "修正" }),
    );
    const multilineInput = screen.getByRole("textbox", {
      name: "编辑第 1 条字幕",
    });
    Object.defineProperty(multilineInput, "scrollHeight", {
      configurable: true,
      value: 96,
    });
    fireEvent.change(multilineInput, {
      target: { value: "修正后的第一句\n补充的第二行" },
    });
    expect(multilineInput).toHaveStyle({ height: "96px" });
    fireEvent.click(
      within(editingCard!).getByRole("button", { name: "保存" }),
    );

    expect(
      screen.getByText("修正后的第一句 补充的第二行"),
    ).toBeInTheDocument();
    expect(screen.getAllByRole("button", { name: "修正" })).toHaveLength(2);
    expect(screen.getAllByRole("button", { name: "删除" })).toHaveLength(2);

    vi.mocked(chooseExportDestination).mockResolvedValue(
      "/tmp/vad-test-cut.wav",
    );
    vi.mocked(exportEditedMedia).mockImplementation(
      async (_source, _destination, _kind, _segments, onProgress) => {
        onProgress({ percent: 92, message: "正在生成字幕文件" });
        return {
          mediaPath: "/tmp/vad-test-cut.wav",
          subtitlePath: "/tmp/vad-test-cut.srt",
        };
      },
    );
    fireEvent.click(screen.getByRole("button", { name: "导出音频" }));

    expect(
      await screen.findByRole("heading", { name: "导出完成" }),
    ).toBeInTheDocument();
    expect(exportEditedMedia).toHaveBeenCalledWith(
      "/tmp/vad-test.wav",
      "/tmp/vad-test-cut.wav",
      "audio",
      expect.arrayContaining([
        expect.objectContaining({
          id: 1,
          editedText: "修正后的第一句\n补充的第二行",
          retained: true,
        }),
      ]),
      expect.any(Function),
    );
  });
});
