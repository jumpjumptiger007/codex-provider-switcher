import { useRef, useState } from "react";
import { asDesktopError, runDoctor } from "../lib/cps";
import type { DesktopError } from "../lib/cps";
import type { DoctorReport, DoctorSeverity } from "../types/safety";

const labels: Record<DoctorSeverity, string> = {
  ok: "OK", info: "Info", warning: "Warning", error: "Error",
};

type DoctorState =
  | { kind: "idle" | "running" }
  | { kind: "success"; report: DoctorReport; revision: number }
  | { kind: "error"; error: DesktopError };

export default function DiagnosticsPanel({ revision, mutationRunning }: {
  revision: number;
  mutationRunning: boolean;
}) {
  const [state, setState] = useState<DoctorState>({ kind: "idle" });
  const inFlight = useRef(false);
  const stale = state.kind === "success" && (state.revision !== revision || mutationRunning);

  async function run() {
    if (inFlight.current) return;
    inFlight.current = true;
    setState({ kind: "running" });
    try {
      const report = await runDoctor();
      setState({ kind: "success", report, revision });
    } catch (reason) {
      setState({ kind: "error", error: asDesktopError(reason) });
    } finally {
      inFlight.current = false;
    }
  }

  return (
    <section className="safety-section" aria-labelledby="diagnostics-title">
      <h2 id="diagnostics-title">Diagnostics</h2>
      <div className="safety-card" aria-busy={state.kind === "running"}>
        <p className="workflow-hint">
          Check the local Codex configuration, provider setup, credentials, and recovery readiness.
          Diagnostics run locally and offline.
        </p>
        <button className="text-button" type="button" disabled={state.kind === "running"} onClick={() => void run()}>
          {state.kind === "running" ? "Running diagnostics…" : "Run diagnostics"}
        </button>
        {state.kind === "running" && <p className="workflow-hint" role="status">Checking local setup…</p>}
        {state.kind === "error" && <p className="workflow-error" role="alert">{state.error.message}</p>}
        {state.kind === "success" && (
          <div className={stale ? "diagnostics-report is-stale" : "diagnostics-report"}>
            {stale ? (
              <p className="workflow-hint" role="status">Local state changed. These are previous findings; run diagnostics again.</p>
            ) : (
              <p className={state.report.hasErrors ? "workflow-error" : "workflow-success"} role="status">
                {state.report.hasErrors ? "Blocking issues found." : "No blocking issues found."}
              </p>
            )}
            <ol className="diagnostic-findings" aria-label={stale ? "Previous diagnostic findings" : "Diagnostic findings"}>
              {state.report.findings.map((finding, index) => (
                <li key={`${index}-${finding.check}`}>
                  <span className={`severity severity--${finding.severity}`}>{labels[finding.severity]}</span>
                  <div><strong>{finding.check}</strong><p>{finding.message}</p></div>
                </li>
              ))}
            </ol>
          </div>
        )}
      </div>
    </section>
  );
}
