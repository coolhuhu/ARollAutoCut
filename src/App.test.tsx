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
  getAudioExportExtension,
  listenForAppClose,
  transcribeMedia,
} from "./media-api";
import {
  downloadModel,
  getModelStatus,
  getVadSettings,
  resetVadSettings,
  saveVadSettings,
} from "./model-api";

vi.mock("./model-api", () => ({
  getModelStatus: vi.fn(),
  chooseModelDirectory: vi.fn(),
  downloadModel: vi.fn(),
  cancelModelDownload: vi.fn(),
  getVadSettings: vi.fn(),
  saveVadSettings: vi.fn(),
  resetVadSettings: vi.fn(),
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
    getAudioExportExtension: vi.fn(),
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
    vi.mocked(getAudioExportExtension).mockResolvedValue("m4a");
    vi.mocked(getModelStatus).mockResolvedValue({
      state: "missing",
      directory: "/App Data/models/sense-voice",
      issues: ["缺少文件：model.int8.onnx"],
    });
    vi.mocked(getVadSettings).mockResolvedValue({
      minSilenceDuration: 0.5,
      minSpeechDuration: 0.25,
      maxSpeechDuration: 20,
    });
    vi.mocked(saveVadSettings).mockImplementation(async (settings) => settings);
    vi.mocked(resetVadSettings).mockResolvedValue({
      minSilenceDuration: 0.5,
      minSpeechDuration: 0.25,
      maxSpeechDuration: 20,
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
    expect(
      await screen.findByRole("heading", { name: "VAD 设置" }),
    ).toBeInTheDocument();
    expect(
      screen.getByLabelText("最短静音时长 min_silence_duration"),
    ).toHaveValue(0.5);
    expect(
      screen.getByLabelText("最短语音时长 min_speech_duration"),
    ).toHaveValue(0.25);
    expect(
      screen.getByLabelText("最长语音时长 max_speech_duration"),
    ).toHaveValue(20);
  });

  it("saves and resets VAD settings in the model settings dialog", async () => {
    vi.mocked(getModelStatus).mockResolvedValue({
      state: "ready",
      directory: "/models/sense-voice",
      issues: [],
    });
    vi.mocked(getVadSettings).mockResolvedValue({
      minSilenceDuration: 0.8,
      minSpeechDuration: 0.4,
      maxSpeechDuration: 30,
    });

    render(<App />);
    await screen.findByText("SenseVoice 模型可用");
    fireEvent.click(screen.getByRole("button", { name: "模型设置" }));
    const minSilence = await screen.findByLabelText(
      "最短静音时长 min_silence_duration",
    );
    const minSpeech = screen.getByLabelText(
      "最短语音时长 min_speech_duration",
    );
    const maxSpeech = screen.getByLabelText(
      "最长语音时长 max_speech_duration",
    );

    expect(minSilence).toHaveValue(0.8);
    fireEvent.change(minSilence, { target: { value: "1.2" } });
    fireEvent.change(minSpeech, { target: { value: "0.6" } });
    fireEvent.change(maxSpeech, { target: { value: "45" } });
    fireEvent.click(screen.getByRole("button", { name: "保存 VAD 设置" }));

    await waitFor(() =>
      expect(saveVadSettings).toHaveBeenCalledWith({
        minSilenceDuration: 1.2,
        minSpeechDuration: 0.6,
        maxSpeechDuration: 45,
      }),
    );
    expect(
      screen.getByText("VAD 设置已保存，将在下一次识别时生效。"),
    ).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "恢复默认值" }));

    await waitFor(() => expect(resetVadSettings).toHaveBeenCalledOnce());
    expect(minSilence).toHaveValue(0.5);
    expect(minSpeech).toHaveValue(0.25);
    expect(maxSpeech).toHaveValue(20);
    expect(
      screen.getByText("已恢复默认 VAD 设置，将在下一次识别时生效。"),
    ).toBeInTheDocument();
  });

  it("shows a validation error for invalid VAD settings", async () => {
    render(<App />);
    await screen.findByText("SenseVoice 模型尚未配置");
    fireEvent.click(screen.getByRole("button", { name: "模型设置" }));
    const minSilence = await screen.findByLabelText(
      "最短静音时长 min_silence_duration",
    );

    fireEvent.change(minSilence, { target: { value: "0.01" } });
    fireEvent.click(screen.getByRole("button", { name: "保存 VAD 设置" }));

    expect(
      screen.getByText(/最短静音时长 min_silence_duration 必须在 0.1 到 5 秒之间/),
    ).toBeInTheDocument();
    expect(saveVadSettings).not.toHaveBeenCalled();
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
        previewAudioPath: "/tmp/vad-test.wav",
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
    const playAudio = vi
      .spyOn(window.HTMLMediaElement.prototype, "play")
      .mockResolvedValue(undefined);
    const pauseAudio = vi
      .spyOn(window.HTMLMediaElement.prototype, "pause")
      .mockImplementation(() => {});

    fireEvent.click(screen.getByRole("button", { name: "播放第 1 条字幕" }));
    expect(playAudio).toHaveBeenCalledOnce();
    expect(
      screen.getByRole("button", { name: "暂停第 1 条字幕" }),
    ).toBeInTheDocument();
    const previewAudio = screen.getByTestId(
      "preview-audio",
    ) as HTMLAudioElement;
    const firstSegmentProgress = screen.getByLabelText("第 1 条字幕播放进度");
    fireEvent.change(firstSegmentProgress, { target: { value: "0.5" } });
    expect(previewAudio.currentTime).toBeCloseTo(0.5);
    fireEvent.click(screen.getByRole("button", { name: "暂停第 1 条字幕" }));
    expect(pauseAudio).toHaveBeenCalled();
    playAudio.mockRestore();
    pauseAudio.mockRestore();

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
      within(editingCard!).getByRole("alert"),
    ).toHaveTextContent("字幕内容不能换行编辑，请删除换行后再保存。");
    expect(multilineInput).toHaveValue("修正后的第一句\n补充的第二行");
    expect(
      within(editingCard!).getByRole("button", { name: "保存" }),
    ).toBeInTheDocument();

    fireEvent.click(
      within(editingCard!).getByRole("button", { name: "取消" }),
    );
    expect(screen.getByText("第一句")).toBeInTheDocument();
    expect(within(editingCard!).queryByRole("alert")).not.toBeInTheDocument();

    fireEvent.click(
      within(editingCard!).getByRole("button", { name: "修正" }),
    );
    const correctedInput = screen.getByRole("textbox", {
      name: "编辑第 1 条字幕",
    });
    fireEvent.change(correctedInput, {
      target: { value: "修正后的第一句 补充的第二行" },
    });
    expect(within(editingCard!).queryByRole("alert")).not.toBeInTheDocument();
    fireEvent.click(
      within(editingCard!).getByRole("button", { name: "保存" }),
    );

    expect(screen.getByText("修正后的第一句 补充的第二行")).toBeInTheDocument();
    expect(screen.getAllByRole("button", { name: "修正" })).toHaveLength(2);
    expect(screen.getAllByRole("button", { name: "删除" })).toHaveLength(2);

    vi.mocked(chooseExportDestination).mockResolvedValue(
      "/tmp/vad-test-cut.wav",
    );
    vi.mocked(exportEditedMedia).mockImplementation(
      async (_source, _destination, _kind, _mode, _segments, onProgress) => {
        onProgress({ percent: 92, message: "正在生成字幕文件" });
        return {
          files: [
            { kind: "audio", path: "/tmp/vad-test-cut.wav" },
            { kind: "subtitle", path: "/tmp/vad-test-cut.srt" },
          ],
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
      "audioWithSubtitle",
      expect.arrayContaining([
        expect.objectContaining({
          id: 1,
          editedText: "修正后的第一句 补充的第二行",
          retained: true,
        }),
      ]),
      expect.any(Function),
    );
    expect(screen.getByText("音频文件")).toBeInTheDocument();
    expect(screen.getByText("字幕文件")).toBeInTheDocument();

    const closeProtection = vi.mocked(listenForAppClose).mock.calls[0]?.[0];
    expect(closeProtection?.()).toBe(true);

    fireEvent.click(screen.getByRole("button", { name: "继续编辑" }));
    expect(
      screen.queryByRole("heading", { name: "导出完成" }),
    ).not.toBeInTheDocument();
    expect(
      screen.getByRole("heading", { name: "保留需要的口播内容" }),
    ).toBeInTheDocument();
    expect(screen.getByText("修正后的第一句 补充的第二行")).toBeInTheDocument();

    vi.mocked(chooseExportDestination).mockResolvedValue(
      "/tmp/vad-test-cut.srt",
    );
    fireEvent.click(screen.getByRole("button", { name: "更多导出选项" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "仅导出字幕" }));

    expect(
      await screen.findByRole("heading", { name: "导出完成" }),
    ).toBeInTheDocument();
    expect(exportEditedMedia).toHaveBeenLastCalledWith(
      "/tmp/vad-test.wav",
      "/tmp/vad-test-cut.srt",
      "audio",
      "subtitleOnly",
      expect.arrayContaining([
        expect.objectContaining({
          id: 1,
          editedText: "修正后的第一句 补充的第二行",
          retained: true,
        }),
      ]),
      expect.any(Function),
    );
    expect(exportEditedMedia).toHaveBeenCalledTimes(2);
  });

  it("keeps the current edit when reupload file selection is cancelled", async () => {
    vi.mocked(getModelStatus).mockResolvedValue({
      state: "ready",
      directory: "/models/sense-voice",
      issues: [],
    });
    vi.mocked(chooseMediaFile).mockResolvedValueOnce("/tmp/vad-test.wav");
    vi.mocked(transcribeMedia).mockResolvedValue({
      sourcePath: "/tmp/vad-test.wav",
      sourceName: "vad-test.wav",
      mediaKind: "audio",
      previewAudioPath: "/tmp/vad-test.wav",
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
          startSample: 16_000,
          endSample: 32_000,
          sampleRate: 16_000,
          originalText: "第二句",
          editedText: "第二句",
          retained: true,
        },
      ],
    });
    vi.spyOn(window, "confirm").mockReturnValue(true);

    render(<App />);
    const upload = await screen.findByRole("button", {
      name: /点击上传或拖拽文件到此处/,
    });
    await waitFor(() => expect(upload).toBeEnabled());
    fireEvent.click(upload);
    await screen.findByRole("heading", { name: "保留需要的口播内容" });

    fireEvent.click(screen.getAllByRole("button", { name: "删除" })[0]);
    expect(screen.getByText(/已保留/)).toHaveTextContent("1 / 2 段");
    vi.mocked(chooseMediaFile).mockResolvedValueOnce(null);

    fireEvent.click(screen.getByRole("button", { name: "重新上传" }));

    await waitFor(() => expect(chooseMediaFile).toHaveBeenCalledTimes(2));
    expect(
      screen.getByRole("heading", { name: "保留需要的口播内容" }),
    ).toBeInTheDocument();
    expect(screen.getByText("第一句")).toBeInTheDocument();
    expect(screen.getByText("第二句")).toBeInTheDocument();
    expect(screen.getByText(/已保留/)).toHaveTextContent("1 / 2 段");
    expect(transcribeMedia).toHaveBeenCalledTimes(1);
  });

  it("exports only subtitles from the audio export menu", async () => {
    vi.mocked(getModelStatus).mockResolvedValue({
      state: "ready",
      directory: "/models/sense-voice",
      issues: [],
    });
    vi.mocked(chooseMediaFile).mockResolvedValue("/tmp/voice.wav");
    vi.mocked(transcribeMedia).mockResolvedValue({
      sourcePath: "/tmp/voice.wav",
      sourceName: "voice.wav",
      mediaKind: "audio",
      previewAudioPath: "/tmp/voice.wav",
      segments: [
        {
          id: 1,
          startSample: 16_000,
          endSample: 32_000,
          sampleRate: 16_000,
          originalText: "保留这一句",
          editedText: "保留这一句",
          retained: true,
        },
      ],
    });
    vi.mocked(chooseExportDestination).mockResolvedValue(
      "/tmp/voice-cut.srt",
    );
    vi.mocked(exportEditedMedia).mockResolvedValue({
      files: [{ kind: "subtitle", path: "/tmp/voice-cut.srt" }],
    });

    render(<App />);
    const upload = await screen.findByRole("button", {
      name: /点击上传或拖拽文件到此处/,
    });
    await waitFor(() => expect(upload).toBeEnabled());
    fireEvent.click(upload);
    await screen.findByRole("heading", { name: "保留需要的口播内容" });

    fireEvent.click(screen.getByRole("button", { name: "更多导出选项" }));
    fireEvent.keyDown(document, { key: "Escape" });
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "更多导出选项" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "仅导出字幕" }));

    expect(
      await screen.findByRole("heading", { name: "导出完成" }),
    ).toBeInTheDocument();
    expect(chooseExportDestination).toHaveBeenCalledWith(
      "/tmp/voice.wav",
      "subtitleOnly",
      undefined,
    );
    expect(exportEditedMedia).toHaveBeenCalledWith(
      "/tmp/voice.wav",
      "/tmp/voice-cut.srt",
      "audio",
      "subtitleOnly",
      expect.any(Array),
      expect.any(Function),
    );
    expect(screen.getByText("字幕文件")).toBeInTheDocument();
    expect(screen.queryByText("音频文件")).not.toBeInTheDocument();
  });

  it("detects the video audio container before exporting audio only", async () => {
    vi.mocked(getModelStatus).mockResolvedValue({
      state: "ready",
      directory: "/models/sense-voice",
      issues: [],
    });
    vi.mocked(chooseMediaFile).mockResolvedValue("/tmp/talking.mov");
    vi.mocked(transcribeMedia).mockResolvedValue({
      sourcePath: "/tmp/talking.mov",
      sourceName: "talking.mov",
      mediaKind: "video",
      previewAudioPath: "/tmp/talking-preview.wav",
      segments: [
        {
          id: 1,
          startSample: 0,
          endSample: 16_000,
          sampleRate: 16_000,
          originalText: "视频内容",
          editedText: "视频内容",
          retained: true,
        },
      ],
    });
    vi.mocked(getAudioExportExtension).mockResolvedValue("m4a");
    vi.mocked(chooseExportDestination).mockResolvedValue(
      "/tmp/talking-cut-audio.m4a",
    );
    vi.mocked(exportEditedMedia).mockResolvedValue({
      files: [{ kind: "audio", path: "/tmp/talking-cut-audio.m4a" }],
    });

    render(<App />);
    const upload = await screen.findByRole("button", {
      name: /点击上传或拖拽文件到此处/,
    });
    await waitFor(() => expect(upload).toBeEnabled());
    fireEvent.click(upload);
    await screen.findByRole("heading", { name: "保留需要的口播内容" });

    fireEvent.click(screen.getByRole("button", { name: "更多导出选项" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "仅导出音频" }));

    await screen.findByRole("heading", { name: "导出完成" });
    expect(getAudioExportExtension).toHaveBeenCalledWith("/tmp/talking.mov");
    expect(chooseExportDestination).toHaveBeenCalledWith(
      "/tmp/talking.mov",
      "audioOnly",
      "m4a",
    );
    expect(exportEditedMedia).toHaveBeenCalledWith(
      "/tmp/talking.mov",
      "/tmp/talking-cut-audio.m4a",
      "video",
      "audioOnly",
      expect.any(Array),
      expect.any(Function),
    );
  });
});
