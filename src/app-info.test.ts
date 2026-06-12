import { describe, expect, it } from "vitest";

import { APP_NAME, supportedFormatsLabel } from "./app-info";

describe("app info", () => {
  it("uses the confirmed product name", () => {
    expect(APP_NAME).toBe("ARollCut");
  });

  it("lists every supported input format", () => {
    expect(supportedFormatsLabel()).toBe(
      "MP4、MOV、WAV、MP3、M4A、AAC、FLAC",
    );
  });
});
