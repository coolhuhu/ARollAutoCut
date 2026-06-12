import { describe, expect, it } from "vitest";

import { defaultExportName, formatMediaTimestamp } from "./media-api";

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
