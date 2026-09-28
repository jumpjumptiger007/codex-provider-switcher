import { useState } from "react";
import type { FormEvent } from "react";
import ProviderMark from "./ProviderMark";
import type { CredentialStatus, Model, Provider } from "../types/provider";
import type { DesktopError } from "../lib/cps";

export interface CredentialOperation {
  providerId: string;
  kind: "checking" | "saving";
}

interface ProviderOperationError extends DesktopError {
  providerId: string;
}

export interface ProviderDetailsProps {
  provider: Provider;
  credentialStatus: CredentialStatus | null;
  credentialOperation: CredentialOperation | null;
  credentialError: ProviderOperationError | null;
  models: Model[] | null;
  modelsLoading: boolean;
  modelsError: ProviderOperationError | null;
  selectedModel: string;
  nativeModel: string;
  switchPending: boolean;
  switchError: ProviderOperationError | null;
  successTarget: string | null;
  onCheckCredential: () => void;
  onSaveCredential: (credential: string) => Promise<void>;
  onLoadModels: () => void;
  onSelectModel: (modelId: string) => void;
  onNativeModelChange: (modelId: string) => void;
  onSwitch: (modelId: string) => void;
}

export default function ProviderDetails({
  provider,
  credentialStatus,
  credentialOperation,
  credentialError,
  models,
  modelsLoading,
  modelsError,
  selectedModel,
  nativeModel,
  switchPending,
  switchError,
  successTarget,
  onCheckCredential,
  onSaveCredential,
  onLoadModels,
  onSelectModel,
  onNativeModelChange,
  onSwitch,
}: ProviderDetailsProps) {
  const isDirect = provider.requiresCredential;
  const isCurrentCredential = credentialStatus?.providerId === provider.id;
  const credentialPresent =
    isCurrentCredential && credentialStatus.status === "present";
  const credentialIsLoading =
    credentialOperation?.providerId === provider.id &&
    credentialOperation.kind === "checking";
  const credentialIsSaving =
    credentialOperation?.providerId === provider.id &&
    credentialOperation.kind === "saving";
  const canLoadModels = credentialPresent && !switchPending;
  const canSwitchDirect =
    canLoadModels &&
    models !== null &&
    selectedModel.length > 0 &&
    !modelsLoading &&
    !switchPending;
  const canSwitchNative = nativeModel.trim().length > 0 && !switchPending;

  return (
    <article className="details-card" aria-live="polite">
      <div className="details-header">
        <ProviderMark providerId={provider.id} />
        <div>
          <h3>{provider.name}</h3>
          <p>{transportDescription(provider.transport)}</p>
        </div>
      </div>

      <dl className="detail-list">
        <div>
          <dt>Compatibility</dt>
          <dd>{compatibilityLabel(provider.compatibility)}</dd>
        </div>
      </dl>

      {isDirect ? (
        <>
          <section className="workflow-panel" aria-labelledby="authentication-title">
            <div className="workflow-heading">
              <h4 id="authentication-title">Authentication</h4>
              {credentialPresent && !credentialError && (
                <span className="credential-state">Credential stored</span>
              )}
            </div>
            <CredentialPanel
              key={provider.id}
              providerId={provider.id}
              checked={isCurrentCredential}
              present={credentialPresent}
              checking={credentialIsLoading}
              saving={credentialIsSaving}
              disabled={switchPending}
              error={credentialError?.providerId === provider.id ? credentialError : null}
              onCheck={onCheckCredential}
              onSave={onSaveCredential}
            />
          </section>

          <section className="workflow-panel" aria-labelledby="models-title">
            <div className="workflow-heading">
              <h4 id="models-title">Models</h4>
              <button
                className="text-button"
                type="button"
                disabled={!canLoadModels || modelsLoading}
                onClick={onLoadModels}
              >
                {modelsLoading
                  ? "Loading models…"
                  : models === null
                    ? "Load models"
                    : "Refresh models"}
              </button>
            </div>
            {!credentialPresent && (
              <p className="workflow-hint">Save an API key to load models.</p>
            )}
            {modelsError?.providerId === provider.id && (
              <p className="workflow-error" role="alert">
                {modelsError.message}
              </p>
            )}
            {models !== null && models.length === 0 && !modelsLoading && (
              <p className="workflow-hint" role="status">
                No models were returned by this provider.
              </p>
            )}
            {models !== null && models.length > 0 && (
              <label className="field-label" htmlFor="provider-model">
                Model
                <select
                  id="provider-model"
                  value={selectedModel}
                  disabled={switchPending}
                  onChange={(event) => onSelectModel(event.currentTarget.value)}
                >
                  <option value="">Choose a model</option>
                  {models.map((model) => (
                    <option key={model.id} value={model.id}>
                      {model.id}
                    </option>
                  ))}
                </select>
              </label>
            )}
            <button
              className="primary-button"
              type="button"
              disabled={!canSwitchDirect}
              onClick={() => onSwitch(selectedModel)}
            >
              {switchPending ? "Switching…" : "Switch to this model"}
            </button>
          </section>
        </>
      ) : (
        <section className="workflow-panel" aria-labelledby="native-model-title">
          <h4 id="native-model-title">Model</h4>
          <p className="workflow-hint">
            Models are managed by Codex. Enter a model ID to switch.
          </p>
          <label className="field-label" htmlFor="native-model-id">
            Model ID
            <input
              id="native-model-id"
              type="text"
              value={nativeModel}
              disabled={switchPending}
              onChange={(event) => onNativeModelChange(event.currentTarget.value)}
              autoComplete="off"
              spellCheck={false}
            />
          </label>
          <button
            className="primary-button"
            type="button"
            disabled={!canSwitchNative}
            onClick={() => onSwitch(nativeModel)}
          >
            {switchPending ? "Switching…" : "Switch model"}
          </button>
        </section>
      )}

      {switchError?.providerId === provider.id && (
        <p className="workflow-error" role="alert">
          {switchError.message}
        </p>
      )}
      {successTarget && (
        <p className="workflow-success" role="status">
          Switched to {successTarget}
        </p>
      )}
    </article>
  );
}

function CredentialPanel({
  providerId,
  checked,
  present,
  checking,
  saving,
  disabled,
  error,
  onCheck,
  onSave,
}: {
  providerId: string;
  checked: boolean;
  present: boolean;
  checking: boolean;
  saving: boolean;
  disabled: boolean;
  error: ProviderOperationError | null;
  onCheck: () => void;
  onSave: (credential: string) => Promise<void>;
}) {
  const [credential, setCredential] = useState("");
  const [replacing, setReplacing] = useState(false);

  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!credential) return;
    try {
      await onSave(credential);
      setCredential("");
      setReplacing(false);
    } catch {
      // Keep the input available for a deliberate retry; the error is shown by the parent.
    }
  }

  if (!checked) {
    return (
      <div className="credential-actions">
        <p className="workflow-hint">Credential status has not been checked yet.</p>
        <button className="text-button" type="button" disabled={checking} onClick={onCheck}>
          {checking ? "Checking…" : "Check credential status"}
        </button>
        {error && <p className="workflow-error" role="alert">{error.message}</p>}
      </div>
    );
  }

  if (present && !replacing) {
    return (
      <div className="credential-actions">
        <button
          className="text-button"
          type="button"
          disabled={disabled || saving}
          onClick={() => setReplacing(true)}
        >
          Replace API key
        </button>
        {error && <p className="workflow-error" role="alert">{error.message}</p>}
      </div>
    );
  }

  return (
    <form className="credential-form" onSubmit={submit}>
      {checked && !present && (
        <p className="workflow-hint">API key required.</p>
      )}
      <label className="field-label" htmlFor={`api-key-${providerId}`}>
        API key
        <input
          id={`api-key-${providerId}`}
          type="password"
          value={credential}
          onChange={(event) => setCredential(event.currentTarget.value)}
          autoComplete="new-password"
          spellCheck={false}
          disabled={disabled || saving}
        />
      </label>
      <div className="credential-actions">
        <button
          className="primary-button"
          type="submit"
          disabled={!credential || disabled || saving}
        >
          {saving ? "Saving…" : "Save API key"}
        </button>
        {replacing && (
          <button
            className="text-button"
            type="button"
            disabled={disabled || saving}
            onClick={() => setReplacing(false)}
          >
            Cancel
          </button>
        )}
        {error && <p className="workflow-error" role="alert">{error.message}</p>}
      </div>
    </form>
  );
}

function transportDescription(transport: Provider["transport"]): string {
  if (transport === "responses") return "Direct API provider";
  if (transport === "native") return "Native provider";
  return "Connected service provider";
}

function compatibilityLabel(compatibility: Provider["compatibility"]): string {
  switch (compatibility) {
    case "verified":
      return "Verified";
    case "verified-basic":
      return "Basic support";
    case "degraded":
      return "Limited support";
    case "unverified":
      return "Unverified";
    case "unsupported":
      return "Unsupported";
  }
}
