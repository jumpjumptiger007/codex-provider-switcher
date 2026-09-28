import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import App from "./App";
import {
  runDoctor,
  restoreConfig,
  getCredentialStatus,
  getModels,
  getProviders,
  getStatus,
  saveCredential,
  switchModel,
} from "./lib/cps";
import type { Provider, Status } from "./types/provider";

vi.mock("./lib/cps", async (importOriginal) => ({
  ...await importOriginal<typeof import("./lib/cps")>(),
  runDoctor: vi.fn(),
  restoreConfig: vi.fn(),
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
  vi.resetAllMocks();
  vi.mocked(runDoctor).mockResolvedValue({ findings: [
    { check: "config", severity: "ok", message: "valid" },
    { check: "provider", severity: "warning", message: "not managed by CPS" },
    { check: "recovery", severity: "info", message: "no CPS recovery backup" },
  ], hasErrors: false });
  vi.mocked(restoreConfig).mockResolvedValue({ restoredFrom: "config.toml.cps-backup-historical", recoveryBackup: "config.toml.cps-backup-recovery" });
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

async function confirmRestore(user: ReturnType<typeof userEvent.setup>) {
  await user.click(screen.getByRole("button", { name: "Restore configuration" }));
  await user.click(screen.getByRole("button", { name: "Restore" }));
}

async function selectNativeModel(user: ReturnType<typeof userEvent.setup>) {
  await user.click(await screen.findByRole("button", { name: /OpenAI/ }));
  await user.type(screen.getByLabelText("Model ID"), "next-model");
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => { resolve = done; });
  return { promise, resolve };
}

describe("Desktop diagnostics and recovery", () => {
  it("does not run Doctor or restore on startup", async () => {
    render(<App />);
    await screen.findByText("openai/gpt-5");
    expect(screen.getByRole("heading", { name: "Diagnostics" })).toBeTruthy();
    expect(screen.getByRole("heading", { name: "Recovery" })).toBeTruthy();
    expect(runDoctor).not.toHaveBeenCalled();
    expect(restoreConfig).not.toHaveBeenCalled();
  });

  it("renders explicit diagnostics in core order with severity labels and nonblocking warnings", async () => {
    const user = userEvent.setup();
    render(<App />);
    await user.click(screen.getByRole("button", { name: "Run diagnostics" }));
    expect(await screen.findByText("No blocking issues found.")).toBeTruthy();
    const findings = screen.getByRole("list", { name: "Diagnostic findings" });
    expect([...findings.querySelectorAll("strong")].map((node) => node.textContent)).toEqual(["config", "provider", "recovery"]);
    expect(screen.getByText("OK")).toBeTruthy();
    expect(screen.getByText("Warning")).toBeTruthy();
    expect(screen.getByText("Info")).toBeTruthy();
    expect(screen.queryByText("Blocking issues found.")).toBeNull();
    expect(runDoctor).toHaveBeenCalledTimes(1);
  });

  it("shows structured errors and prevents duplicate Doctor requests without blocking switches", async () => {
    const user = userEvent.setup();
    const pending = deferred<Awaited<ReturnType<typeof runDoctor>>>();
    vi.mocked(runDoctor).mockReturnValueOnce(pending.promise);
    render(<App />);
    await selectNativeModel(user);
    await user.click(screen.getByRole("button", { name: "Run diagnostics" }));
    const running = screen.getByRole("button", { name: "Running diagnostics…" }) as HTMLButtonElement;
    expect(running.disabled).toBe(true);
    await user.click(running);
    expect(runDoctor).toHaveBeenCalledTimes(1);
    expect((screen.getByRole("button", { name: "Switch model" }) as HTMLButtonElement).disabled).toBe(false);
    pending.resolve({ findings: [{ check: "config", severity: "error", message: "invalid TOML" }], hasErrors: true });
    expect(await screen.findByText("Blocking issues found.")).toBeTruthy();
    expect(screen.getByText("Error")).toBeTruthy();
    expect(screen.getByText("invalid TOML")).toBeTruthy();
  });

  it("shows safe generic diagnostics errors", async () => {
    const user = userEvent.setup();
    vi.mocked(runDoctor).mockRejectedValueOnce(new Error("private /Users/path"));
    render(<App />);
    await user.click(screen.getByRole("button", { name: "Run diagnostics" }));
    expect(await screen.findByRole("alert")).toHaveProperty("textContent", "This operation could not be completed. Try again.");
    expect(screen.queryByText(/private \/Users/)).toBeNull();
  });

  it("requires final confirmation and Cancel does not restore", async () => {
    const user = userEvent.setup();
    render(<App />);
    await user.click(screen.getByRole("button", { name: "Restore configuration" }));
    expect(screen.getByText("Restore the latest backup?")).toBeTruthy();
    expect(screen.getByText(/Your current configuration will first be saved/)).toBeTruthy();
    expect(restoreConfig).not.toHaveBeenCalled();
    await user.click(screen.getByRole("button", { name: "Cancel" }));
    expect(screen.queryByText("Restore the latest backup?")).toBeNull();
    expect(restoreConfig).not.toHaveBeenCalled();
  });

  it("restores once, shows safe filenames, refreshes status and active badge, and preserves inspected provider", async () => {
    const user = userEvent.setup();
    vi.mocked(getStatus).mockResolvedValueOnce(initialStatus).mockResolvedValueOnce(switchedStatus("deepseek", "restored-model"));
    render(<App />);
    await screen.findByText("openai/gpt-5");
    await confirmRestore(user);
    expect(await screen.findByText("Configuration restored.")).toBeTruthy();
    expect(screen.getByText("config.toml.cps-backup-historical")).toBeTruthy();
    expect(screen.getByText("config.toml.cps-backup-recovery")).toBeTruthy();
    expect(await screen.findByText("deepseek/restored-model")).toBeTruthy();
    expect(restoreConfig).toHaveBeenCalledTimes(1);
    expect(getStatus).toHaveBeenCalledTimes(2);
    expect(runDoctor).not.toHaveBeenCalled();
    const openai = screen.getByRole("button", { name: /OpenAI/ });
    const deepseek = screen.getByRole("button", { name: /DeepSeek/ });
    expect(openai.getAttribute("aria-pressed")).toBe("true");
    expect(openai.textContent).not.toContain("Active");
    expect(deepseek.textContent).toContain("Active");
  });

  it("retains restore success when the independent status refresh fails and removes stale active badges", async () => {
    const user = userEvent.setup();
    vi.mocked(getStatus).mockResolvedValueOnce(initialStatus).mockRejectedValueOnce({ code: "config_invalid", message: "The restored configuration has no active selection." });
    render(<App />);
    await screen.findByText("openai/gpt-5");
    await confirmRestore(user);
    expect(await screen.findByText("Current setup is unavailable")).toBeTruthy();
    expect(screen.getByText("Configuration restored.")).toBeTruthy();
    expect(screen.getByText("config.toml.cps-backup-recovery")).toBeTruthy();
    expect(screen.queryByText(/restore failed/i)).toBeNull();
    expect(screen.queryByText("openai/gpt-5")).toBeNull();
    expect(screen.getByRole("button", { name: /OpenAI/ }).textContent).not.toContain("Active");
  });

  it("keeps previous Current setup after a failed restore", async () => {
    const user = userEvent.setup();
    vi.mocked(restoreConfig).mockRejectedValueOnce({ code: "no_recovery_backup", message: "No CPS recovery backup was found." });
    render(<App />);
    await screen.findByText("openai/gpt-5");
    await confirmRestore(user);
    expect(await screen.findByText("No CPS recovery backup was found.")).toBeTruthy();
    expect(screen.getByText("openai/gpt-5")).toBeTruthy();
    expect(getStatus).toHaveBeenCalledTimes(1);
    expect(screen.queryByText("Configuration restored.")).toBeNull();
  });

  it("prevents duplicate restore and model mutation while restoring and permits provider navigation", async () => {
    const user = userEvent.setup();
    const pending = deferred<Awaited<ReturnType<typeof restoreConfig>>>();
    vi.mocked(restoreConfig).mockReturnValueOnce(pending.promise);
    render(<App />);
    await selectNativeModel(user);
    await confirmRestore(user);
    const running = screen.getByRole("button", { name: "Restoring…" }) as HTMLButtonElement;
    expect(running.disabled).toBe(true);
    await user.click(running);
    const switchButton = screen.getByRole("button", { name: "Switch model" }) as HTMLButtonElement;
    expect(switchButton.disabled).toBe(true);
    await user.click(switchButton);
    expect(switchModel).not.toHaveBeenCalled();
    expect(restoreConfig).toHaveBeenCalledTimes(1);
    await user.click(screen.getByRole("button", { name: /DeepSeek/ }));
    expect(screen.getByRole("button", { name: /DeepSeek/ }).getAttribute("aria-pressed")).toBe("true");
    pending.resolve({ restoredFrom: "config.toml.cps-backup-old", recoveryBackup: "config.toml.cps-backup-new" });
    await screen.findByText("Configuration restored.");
  });

  it("prevents restore while switching, including a confirmation already open", async () => {
    const user = userEvent.setup();
    const pending = deferred<Status>();
    vi.mocked(switchModel).mockReturnValueOnce(pending.promise);
    render(<App />);
    await selectNativeModel(user);
    await user.click(screen.getByRole("button", { name: "Restore configuration" }));
    await user.click(screen.getByRole("button", { name: "Switch model" }));
    const restore = screen.getByRole("button", { name: "Restore" }) as HTMLButtonElement;
    expect(restore.disabled).toBe(true);
    await user.click(restore);
    expect(restoreConfig).not.toHaveBeenCalled();
    pending.resolve(switchedStatus("openai", "next-model"));
    await screen.findByText("Switched to openai/next-model");
  });

  it("marks Doctor stale after mutation and clears switch success after restore without rerunning Doctor", async () => {
    const user = userEvent.setup();
    vi.mocked(switchModel).mockResolvedValueOnce(switchedStatus("openai", "next-model"));
    render(<App />);
    await selectNativeModel(user);
    await user.click(screen.getByRole("button", { name: "Run diagnostics" }));
    await screen.findByText("No blocking issues found.");
    await user.click(screen.getByRole("button", { name: "Switch model" }));
    await screen.findByText("Switched to openai/next-model");
    expect(screen.getByText(/These are previous findings/)).toBeTruthy();
    expect(screen.queryByText("No blocking issues found.")).toBeNull();
    await confirmRestore(user);
    await screen.findByText("Configuration restored.");
    expect(screen.queryByText("Switched to openai/next-model")).toBeNull();
    expect(runDoctor).toHaveBeenCalledTimes(1);
  });

  it("marks an in-flight Doctor response stale if restore changed config", async () => {
    const user = userEvent.setup();
    const pending = deferred<Awaited<ReturnType<typeof runDoctor>>>();
    vi.mocked(runDoctor).mockReturnValueOnce(pending.promise);
    render(<App />);
    await screen.findByText("openai/gpt-5");
    await user.click(screen.getByRole("button", { name: "Run diagnostics" }));
    await confirmRestore(user);
    await screen.findByText("Configuration restored.");
    pending.resolve({ findings: [{ check: "config", severity: "ok", message: "valid" }], hasErrors: false });
    expect(await screen.findByText(/These are previous findings/)).toBeTruthy();
    expect(screen.queryByText("No blocking issues found.")).toBeNull();
  });
});
