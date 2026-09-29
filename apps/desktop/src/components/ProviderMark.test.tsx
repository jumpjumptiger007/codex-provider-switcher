import { cleanup, render } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import ProviderMark from "./ProviderMark";

afterEach(cleanup);

describe("ProviderMark", () => {
  it.each([
    ["deepseek", "DS"],
    ["lmstudio", "LM"],
    ["ollama", "OL"],
    ["openai", "OA"],
    ["openrouter", "OR"],
    ["xai", "XA"],
    ["unknown-provider", "UN"],
  ])("keeps initials for %s", (providerId, initials) => {
    const { container } = render(<ProviderMark providerId={providerId} />);
    const mark = container.firstElementChild;

    expect(mark?.getAttribute("aria-hidden")).toBe("true");
    expect(mark?.textContent).toBe(initials);
    expect(mark?.querySelector("img")).toBeNull();
  });
});
