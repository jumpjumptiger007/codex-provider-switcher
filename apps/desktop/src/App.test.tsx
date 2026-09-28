import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import App from "./App";
import {
  getCredentialStatus,
  getModels,
  getProviders,
  getStatus,
  saveCredential,
  switchModel,
} from "./lib/cps";
import type { Provider, Status } from "./types/provider";

vi.mock("./lib/cps", () => ({
  getCredentialStatus: vi.fn(),
  getModels: vi.fn(),
  getProviders: vi.fn(),
  getStatus: vi.fn(),
  saveCredential: vi.fn(),
  switchModel: vi.fn(),
}));

const providers: Provider[] = [
  {
    id: "deepseek",
    name: "DeepSeek",
    transport: "responses",
    compatibility: "verified-basic",
    discovery: "provider-models",
    requiresCredential: true,
  },
  {
    id: "openai",
    name: "OpenAI",
    transport: "native",
    compatibility: "verified",
    discovery: "codex-managed",
    requiresCredential: false,
  },
];

const initialStatus: Status = {
  provider: "openai",
  model: "gpt-5",
  target: "openai/gpt-5",
  knownProvider: true,
  transport: "native",
};

function switchedStatus(provider: string, model: string): Status {
  return {
    provider,
    model,
    target: `${provider}/${model}`,
    knownProvider: true,
    transport: provider === "openai" ? "native" : "responses",
  };
}

beforeEach(() => {
  vi.mocked(getProviders).mockResolvedValue(providers);
  vi.mocked(getStatus).mockResolvedValue(initialStatus);
  vi.mocked(getCredentialStatus).mockResolvedValue({
    providerId: "deepseek",
    status: "missing",
  });
  vi.mocked(saveCredential).mockResolvedValue({
    providerId: "deepseek",
    status: "present",
  });
  vi.mocked(getModels).mockResolvedValue([
    {
      id: "anthropic/claude-sonnet-4",
      target: "deepseek/anthropic/claude-sonnet-4",
    },
  ]);
  vi.mocked(switchModel).mockResolvedValue(
    switchedStatus("deepseek", "anthropic/claude-sonnet-4"),
  );
});

afterEach(() => cleanup());

describe("Desktop provider workflow", () => {
  it("saves a missing credential, loads models, selects one, and updates current setup after switching", async () => {
    const user = userEvent.setup();
    render(<App />);

    await user.click(await screen.findByRole("button", { name: /DeepSeek/ }));
    const password = await screen.findByLabelText("API key") as HTMLInputElement;
    expect(password.type).toBe("password");

    await user.type(password, "temporary-test-key");
    await user.click(screen.getByRole("button", { name: "Save API key" }));
    await waitFor(() => expect(screen.getByText("Credential stored")).toBeTruthy());
    expect(screen.queryByLabelText("API key")).toBeNull();

    await user.click(screen.getByRole("button", { name: "Replace API key" }));
    expect((screen.getByLabelText("API key") as HTMLInputElement).value).toBe("");

    await user.click(screen.getByRole("button", { name: "Cancel" }));
    await user.click(screen.getByRole("button", { name: "Load models" }));
    await screen.findByRole("option", { name: "anthropic/claude-sonnet-4" });

    const switchButton = screen.getByRole("button", { name: "Switch to this model" }) as HTMLButtonElement;
    expect(switchButton.disabled).toBe(true);
    await user.selectOptions(screen.getByLabelText("Model"), "anthropic/claude-sonnet-4");
    expect(switchButton.disabled).toBe(false);
    await user.click(switchButton);

    expect(await screen.findByText("deepseek/anthropic/claude-sonnet-4")).toBeTruthy();
    expect(await screen.findByText("Switched to deepseek/anthropic/claude-sonnet-4")).toBeTruthy();
    expect(vi.mocked(switchModel)).toHaveBeenCalledWith(
      "deepseek",
      "anthropic/claude-sonnet-4",
    );
  });

  it("shows a manual model field for native providers without credential controls", async () => {
    const user = userEvent.setup();
    vi.mocked(switchModel).mockResolvedValueOnce(
      switchedStatus("openai", "local/model:latest"),
    );
    render(<App />);

    await user.click(await screen.findByRole("button", { name: /OpenAI/ }));
    const modelField = screen.getByLabelText("Model ID") as HTMLInputElement;
    expect(modelField).toBeTruthy();
    expect(screen.queryByLabelText("API key")).toBeNull();
    expect(screen.queryByText("Authentication")).toBeNull();
    expect(getCredentialStatus).not.toHaveBeenCalled();
    await user.type(modelField, "local/model:latest");
    const switchButton = screen.getByRole("button", { name: "Switch model" }) as HTMLButtonElement;
    expect(switchButton.disabled).toBe(false);
    await user.click(switchButton);
    expect(await screen.findByText("openai/local/model:latest")).toBeTruthy();
    expect(switchModel).toHaveBeenCalledWith("openai", "local/model:latest");
  });

  it("keeps current setup unchanged and shows an error after a failed switch", async () => {
    const user = userEvent.setup();
    vi.mocked(switchModel).mockRejectedValueOnce({
      code: "config_conflict",
      message: "Codex configuration changed outside CPS. Refresh the current setup before trying again.",
    });
    render(<App />);

    await user.click(await screen.findByRole("button", { name: /DeepSeek/ }));
    await screen.findByLabelText("API key");
    await user.type(screen.getByLabelText("API key"), "temporary-test-key");
    await user.click(screen.getByRole("button", { name: "Save API key" }));
    await screen.findByText("Credential stored");
    await user.click(screen.getByRole("button", { name: "Load models" }));
    await screen.findByRole("option", { name: "anthropic/claude-sonnet-4" });
    await user.selectOptions(screen.getByLabelText("Model"), "anthropic/claude-sonnet-4");
    await user.click(screen.getByRole("button", { name: "Switch to this model" }));

    expect(await screen.findByRole("alert")).toBeTruthy();
    expect(screen.getByText("openai/gpt-5")).toBeTruthy();
    expect((screen.getByRole("button", { name: /DeepSeek/ }) as HTMLButtonElement)
      .getAttribute("aria-pressed")).toBe("true");
  });

  it("ignores a credential response after selection changes to another provider", async () => {
    const user = userEvent.setup();
    let resolveCredential!: (value: { providerId: string; status: "missing" }) => void;
    vi.mocked(getCredentialStatus).mockReturnValueOnce(
      new Promise((resolve) => { resolveCredential = resolve; }),
    );
    render(<App />);

    await user.click(await screen.findByRole("button", { name: /DeepSeek/ }));
    await user.click(screen.getByRole("button", { name: /OpenAI/ }));
    resolveCredential({ providerId: "deepseek", status: "missing" });

    expect(await screen.findByLabelText("Model ID")).toBeTruthy();
    await waitFor(() => expect(screen.queryByLabelText("API key")).toBeNull());
    expect(getCredentialStatus).toHaveBeenCalledTimes(1);
  });
});
