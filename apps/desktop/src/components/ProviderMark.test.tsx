import { cleanup, render } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import ProviderMark from "./ProviderMark";

afterEach(cleanup);

describe("ProviderMark", () => {
  it.each([
    ["lmstudio"],
    ["openai"],
    ["openrouter"],
  ])("renders the local official mark for %s", (providerId) => {
    const { container } = render(<ProviderMark providerId={providerId} />);
    const mark = container.firstElementChild;
    const image = mark?.querySelector("img");

    expect(mark?.getAttribute("aria-hidden")).toBe("true");
    expect(image?.getAttribute("alt")).toBe("");
    expect(image?.getAttribute("src")).toMatch(/^data:image\/svg\+xml,/);
    expect(image?.getAttribute("src")).not.toMatch(/^https?:/);
    expect(decodeURIComponent(image?.getAttribute("src")?.split(",", 2)[1] ?? ""))
      .toMatch(/^<svg\b/);
  });

  it.each([
    ["deepseek", "DS"],
    ["ollama", "OL"],
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
