import { useState } from "react";
import { asDesktopError, restoreConfig } from "../lib/cps";
import type { DesktopError } from "../lib/cps";
import type { RestoreResult } from "../types/safety";

type RestoreState =
  | { kind: "idle" | "confirming" | "running" }
  | { kind: "success"; result: RestoreResult }
  | { kind: "error"; error: DesktopError };

export default function RecoveryPanel({ mutationRunning, beginRestore, endRestore, onRestored }: {
  mutationRunning: boolean;
  beginRestore: () => boolean;
  endRestore: () => void;
  onRestored: () => void;
}) {
  const [state, setState] = useState<RestoreState>({ kind: "idle" });

  async function restore() {
    if (state.kind !== "confirming" || !beginRestore()) return;
    setState({ kind: "running" });
    try {
      const result = await restoreConfig();
      setState({ kind: "success", result });
    } catch (reason) {
      setState({ kind: "error", error: asDesktopError(reason) });
      return;
    } finally {
      endRestore();
    }
    // Refresh is a separate operation, with its own error display in Current setup.
    onRestored();
  }

  return (
    <section className="safety-section" aria-labelledby="recovery-title">
      <h2 id="recovery-title">Recovery</h2>
      <div className="safety-card" aria-busy={state.kind === "running"}>
        <p className="workflow-hint">Restore the latest valid CPS backup. Restore affects only the Codex configuration.</p>
        {state.kind === "confirming" ? (
          <div className="restore-confirmation" role="group" aria-labelledby="restore-confirmation-title">
            <h3 id="restore-confirmation-title">Restore the latest backup?</h3>
            <p className="workflow-hint">
              Your current configuration will first be saved as a new recovery backup, so this operation can be reversed.
            </p>
            <div className="credential-actions">
              <button className="text-button" type="button" onClick={() => setState({ kind: "idle" })}>Cancel</button>
              <button className="text-button" type="button" disabled={mutationRunning} onClick={() => void restore()}>Restore</button>
            </div>
          </div>
        ) : (
          <button className="text-button" type="button" disabled={mutationRunning || state.kind === "running"} onClick={() => setState({ kind: "confirming" })}>
            {state.kind === "running" ? "Restoring…" : "Restore configuration"}
          </button>
        )}
        {state.kind === "running" && <p className="workflow-hint" role="status">Saving a recovery backup and restoring configuration…</p>}
        {state.kind === "error" && <p className="workflow-error" role="alert">{state.error.message}</p>}
        {state.kind === "success" && (
          <div className="restore-result">
            <p className="workflow-success" role="status">Configuration restored.</p>
            <dl>
              <div><dt>Restored from:</dt><dd><code>{state.result.restoredFrom}</code></dd></div>
              <div><dt>Recovery backup:</dt><dd><code>{state.result.recoveryBackup}</code></dd></div>
            </dl>
          </div>
        )}
      </div>
    </section>
  );
}
