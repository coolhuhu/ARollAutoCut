import { describe, expect, it } from "vitest";

import { formatDownloadBytes } from "./model-api";

describe("formatDownloadBytes", () => {
  it("formats known download totals", () => {
    expect(formatDownloadBytes(512, 1024)).toBe("512 B / 1.0 KB");
    expect(formatDownloadBytes(1_572_864, 3_145_728)).toBe(
      "1.5 MB / 3.0 MB",
    );
  });

  it("formats progress when the server omits content length", () => {
    expect(formatDownloadBytes(2_147_483_648, null)).toBe("2.0 GB");
  });
});
