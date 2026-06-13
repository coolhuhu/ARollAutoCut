import { describe, expect, it, vi } from "vitest";

import {
  defaultExportName,
  formatMediaTimestamp,
  handleAppCloseRequest,
  shouldProtectAppClose,
} from "./media-api";

describe("formatMediaTimestamp", () => {
  it("formats sample positions as SRT timestamps", () => {
    expect(formatMediaTimestamp(0, 16_000)).toBe("00:00:00,000");
    expect(formatMediaTimestamp(24_000, 16_000)).toBe("00:00:01,500");
    expect(formatMediaTimestamp(59_600_000, 16_000)).toBe("01:02:05,000");
  });

  it("floors sub-millisecond sample positions", () => {
    expect(formatMediaTimestamp(1, 48_000)).toBe("00:00:00,000");
  });
});

describe("defaultExportName", () => {
  it("keeps the source container extension", () => {
    expect(defaultExportName("/Users/test/My Clip.MOV")).toBe("My Clip-cut.MOV");
    expect(defaultExportName("C:\\Media\\voice.wav")).toBe("voice-cut.wav");
  });

  it("supports files without an extension", () => {
    expect(defaultExportName("/tmp/recording")).toBe("recording-cut");
  });
});

describe("shouldProtectAppClose", () => {
  it("protects active model, recognition, export, and editing work", () => {
    const idle = {
      isDownloadingModel: false,
      isProcessing: false,
      isExporting: false,
      isEditing: false,
      exportCompleted: false,
    };

    expect(
      shouldProtectAppClose({ ...idle, isDownloadingModel: true }),
    ).toBe(true);
    expect(shouldProtectAppClose({ ...idle, isProcessing: true })).toBe(true);
    expect(shouldProtectAppClose({ ...idle, isExporting: true })).toBe(true);
    expect(shouldProtectAppClose({ ...idle, isEditing: true })).toBe(true);
  });

  it("does not protect an idle or completed session", () => {
    expect(
      shouldProtectAppClose({
        isDownloadingModel: false,
        isProcessing: false,
        isExporting: false,
        isEditing: false,
        exportCompleted: false,
      }),
    ).toBe(false);
    expect(
      shouldProtectAppClose({
        isDownloadingModel: false,
        isProcessing: false,
        isExporting: false,
        isEditing: true,
        exportCompleted: true,
      }),
    ).toBe(false);
  });
});

describe("handleAppCloseRequest", () => {
  it("uses a native confirmation and destroys the window when confirmed", async () => {
    const event = { preventDefault: vi.fn() };
    const confirmClose = vi.fn().mockResolvedValue(true);
    const destroyWindow = vi.fn().mockResolvedValue(undefined);

    await handleAppCloseRequest(
      event,
      false,
      confirmClose,
      destroyWindow,
    );

    expect(event.preventDefault).toHaveBeenCalledOnce();
    expect(confirmClose).toHaveBeenCalledWith(
      "是否关闭 ARollCut？",
      expect.objectContaining({
        title: "关闭 ARollCut",
        okLabel: "关闭",
        cancelLabel: "取消",
      }),
    );
    expect(destroyWindow).toHaveBeenCalledOnce();
  });

  it("keeps the window open when closing unfinished work is cancelled", async () => {
    const event = { preventDefault: vi.fn() };
    const confirmClose = vi.fn().mockResolvedValue(false);
    const destroyWindow = vi.fn().mockResolvedValue(undefined);

    await handleAppCloseRequest(
      event,
      true,
      confirmClose,
      destroyWindow,
    );

    expect(event.preventDefault).toHaveBeenCalledOnce();
    expect(confirmClose).toHaveBeenCalledWith(
      "当前操作尚未完成，关闭 App 将放弃本次编辑。是否继续？",
      expect.any(Object),
    );
    expect(destroyWindow).not.toHaveBeenCalled();
  });
});
