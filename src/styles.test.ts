import { afterEach, describe, expect, it } from "vitest";

import styles from "./styles.css?raw";

describe("subtitle editor styles", () => {
  afterEach(() => {
    document.head.querySelector("[data-test-styles]")?.remove();
    document.body.replaceChildren();
  });

  it("keeps editing text the same size as displayed subtitle text", () => {
    const style = document.createElement("style");
    style.dataset.testStyles = "true";
    style.textContent = styles;
    document.head.append(style);

    const segmentBody = document.createElement("div");
    segmentBody.className = "segment-body";
    segmentBody.innerHTML = `
      <p>普通字幕文字</p>
      <div class="edit-row">
        <textarea class="segment-text-input">编辑中的字幕文字</textarea>
      </div>
    `;
    document.body.append(segmentBody);

    const displayText = segmentBody.querySelector("p");
    const editingInput = segmentBody.querySelector("textarea");
    expect(displayText).not.toBeNull();
    expect(editingInput).not.toBeNull();

    const displayStyle = getComputedStyle(displayText!);
    const editingStyle = getComputedStyle(editingInput!);
    expect(editingStyle.fontSize).toBe(displayStyle.fontSize);
    expect(editingStyle.lineHeight).toBe(displayStyle.lineHeight);
    expect(editingStyle.width).toBe("100%");
    expect(editingStyle.resize).toBe("none");
    expect(editingStyle.overflow).toBe("hidden");
  });
});
