import { useCallback, useEffect, useRef, useState } from "react";
import {
  getCredentialStatus,
  getModels,
  getProviders,
  getStatus,
  saveCredential,
  switchModel,
} from "./lib/cps";
import type { DesktopError } from "./lib/cps";
import ProviderMark from "./components/ProviderMark";
import ProviderDetails from "./components/ProviderDetails";
import type { CredentialOperation } from "./components/ProviderDetails";
import type { CredentialStatus, Model, Provider, Status } from "./types/provider";

interface ProviderError extends DesktopError {
  providerId: string;
}

export default function App() {
  const [providers, setProviders] = useState<Provider[]>([]);
  const [providersLoading, setProvidersLoading] = useState(true);
  const [providersError, setProvidersError] = useState(false);
  const [status, setStatus] = useState<Status | null>(null);
  const [statusLoading, setStatusLoading] = useState(true);
  const [statusError, setStatusError] = useState<DesktopError | null>(null);
  const [selectedProvider, setSelectedProvider] = useState<string | null>(null);
  const [credentialStatus, setCredentialStatus] =
    useState<CredentialStatus | null>(null);
  const [credentialOperation, setCredentialOperation] =
    useState<CredentialOperation | null>(null);
  const [credentialError, setCredentialError] = useState<ProviderError | null>(null);
  const [modelsState, setModelsState] = useState<{
    providerId: string;
    models: Model[];
  } | null>(null);
  const [modelsOperation, setModelsOperation] = useState<string | null>(null);
  const [modelsError, setModelsError] = useState<ProviderError | null>(null);
  const [selectedModel, setSelectedModel] = useState<{
    providerId: string;
    modelId: string;
  } | null>(null);
  const [nativeModels, setNativeModels] = useState<Record<string, string>>({});
  const [switchOperation, setSwitchOperation] = useState<string | null>(null);
  const [switchError, setSwitchError] = useState<ProviderError | null>(null);
  const [switchSuccess, setSwitchSuccess] = useState<{
    providerId: string;
    target: string;
  } | null>(null);

  const selectedByUser = useRef(false);
  const selectedProviderRef = useRef<string | null>(null);
  const providerSnapshot = useRef<Provider[] | null>(null);
  const statusSnapshot = useRef<Status | null>(null);
  const credentialRequest = useRef(0);
  const modelsRequest = useRef(0);
  const switchRequest = useRef(0);
  const switchInFlight = useRef(false);
  const statusRequest = useRef(0);

  const loadProviders = useCallback(() => {
    setProvidersLoading(true);
    setProvidersError(false);
    getProviders()
      .then((inventory) => {
        providerSnapshot.current = inventory;
        setProviders(inventory);
        if (!selectedByUser.current) {
          const activeProvider = statusSnapshot.current?.knownProvider
            ? inventory.find(
                (provider) => provider.id === statusSnapshot.current?.provider,
              )
            : undefined;
          const initialProvider = activeProvider?.id ?? inventory[0]?.id ?? null;
          selectedProviderRef.current = initialProvider;
          setSelectedProvider(initialProvider);
        }
      })
      .catch(() => setProvidersError(true))
      .finally(() => setProvidersLoading(false));
  }, []);

  const loadStatus = useCallback(() => {
    const requestId = ++statusRequest.current;
    setStatusLoading(true);
    setStatusError(null);
    getStatus()
      .then((current) => {
        if (statusRequest.current !== requestId) return;
        statusSnapshot.current = current;
        setStatus(current);
        if (!selectedByUser.current) {
          const activeProvider = current.knownProvider
            ? providerSnapshot.current?.find(
                (provider) => provider.id === current.provider,
              )
            : undefined;
          const initialProvider =
            activeProvider?.id ?? providerSnapshot.current?.[0]?.id ?? null;
          selectedProviderRef.current = initialProvider;
          setSelectedProvider(initialProvider);
        }
      })
      .catch((reason: DesktopError) => {
        if (statusRequest.current === requestId) setStatusError(reason);
      })
      .finally(() => {
        if (statusRequest.current === requestId) setStatusLoading(false);
      });
  }, []);

  useEffect(() => {
    loadProviders();
    loadStatus();
  }, [loadProviders, loadStatus]);

  const inspectCredential = useCallback(async (providerId: string) => {
    if (selectedProviderRef.current !== providerId) return;
    const requestId = ++credentialRequest.current;
    setCredentialOperation({ providerId, kind: "checking" });
    setCredentialError(null);
    try {
      const result = await getCredentialStatus(providerId);
      if (
        credentialRequest.current === requestId &&
        selectedProviderRef.current === providerId
      ) {
        setCredentialStatus(result);
      }
    } catch (reason) {
      if (
        credentialRequest.current === requestId &&
        selectedProviderRef.current === providerId
      ) {
        setCredentialError({ providerId, ...asDesktopError(reason) });
      }
    } finally {
      if (credentialRequest.current === requestId) setCredentialOperation(null);
    }
  }, []);

  const saveProviderCredential = useCallback(
    async (providerId: string, credential: string) => {
      if (selectedProviderRef.current !== providerId) return;
      const requestId = ++credentialRequest.current;
      setCredentialOperation({ providerId, kind: "saving" });
      setCredentialError(null);
      try {
        const result = await saveCredential(providerId, credential);
        if (
          credentialRequest.current === requestId &&
          selectedProviderRef.current === providerId
        ) {
          setCredentialStatus(result);
        }
      } catch (reason) {
        if (
          credentialRequest.current === requestId &&
          selectedProviderRef.current === providerId
        ) {
          const safeError = asDesktopError(reason);
          setCredentialError({ providerId, ...safeError });
          throw safeError;
        }
      } finally {
        if (credentialRequest.current === requestId) setCredentialOperation(null);
      }
    },
    [],
  );

  const loadProviderModels = useCallback(async (providerId: string) => {
    if (selectedProviderRef.current !== providerId) return;
    const requestId = ++modelsRequest.current;
    setModelsOperation(providerId);
    setModelsError(null);
    try {
      const models = await getModels(providerId);
      if (
        modelsRequest.current !== requestId ||
        selectedProviderRef.current !== providerId
      ) {
        return;
      }
      setModelsState({ providerId, models });
      setSelectedModel((current) => {
        const keepSelected =
          current?.providerId === providerId &&
          models.some((model) => model.id === current.modelId);
        return keepSelected ? current : { providerId, modelId: "" };
      });
    } catch (reason) {
      if (
        modelsRequest.current === requestId &&
        selectedProviderRef.current === providerId
      ) {
        const safeError = asDesktopError(reason);
        setModelsError({ providerId, ...safeError });
        if (safeError.code === "credential_missing") {
          setCredentialStatus({ providerId, status: "missing" });
        }
      }
    } finally {
      if (modelsRequest.current === requestId) setModelsOperation(null);
    }
  }, []);

  const switchProviderModel = useCallback(
    async (providerId: string, modelId: string) => {
      if (
        selectedProviderRef.current !== providerId ||
        switchInFlight.current
      ) {
        return;
      }
      switchInFlight.current = true;
      const requestId = ++switchRequest.current;
      setSwitchOperation(providerId);
      setSwitchError(null);
      setSwitchSuccess(null);
      try {
        const refreshedStatus = await switchModel(providerId, modelId);
        if (switchRequest.current !== requestId) return;
        statusRequest.current += 1;
        statusSnapshot.current = refreshedStatus;
        setStatus(refreshedStatus);
        setStatusError(null);
        setStatusLoading(false);
        setSwitchSuccess({ providerId, target: refreshedStatus.target });
      } catch (reason) {
        if (switchRequest.current === requestId) {
          const safeError = asDesktopError(reason);
          setSwitchError({ providerId, ...safeError });
          if (safeError.code === "credential_missing") {
            setCredentialStatus({ providerId, status: "missing" });
            setCredentialError({ providerId, ...safeError });
          }
        }
      } finally {
        if (switchRequest.current === requestId) {
          switchInFlight.current = false;
          setSwitchOperation(null);
        }
      }
    },
    [],
  );

  function handleProviderSelect(provider: Provider) {
    selectedByUser.current = true;
    selectedProviderRef.current = provider.id;
    setSelectedProvider(provider.id);
    credentialRequest.current += 1;
    modelsRequest.current += 1;
    setCredentialOperation(null);
    setModelsOperation(null);
    setCredentialError(null);
    setModelsError(null);
    setSwitchError(null);
    setSwitchSuccess(null);
    if (provider.requiresCredential) void inspectCredential(provider.id);
  }

  const selected = providers.find((provider) => provider.id === selectedProvider);
  const activeProvider = status?.knownProvider
    ? providers.find((provider) => provider.id === status.provider)
    : undefined;
  const visibleModels = modelsState && selected && modelsState.providerId === selected.id
    ? modelsState.models
    : null;

  return (
    <main className="page">
      <header className="page-header">
        <h1>Codex Provider Switcher</h1>
        <p className="subtitle">
          Switch Codex providers and models without editing configuration by hand.
        </p>
      </header>

      <section className="current-section" aria-labelledby="current-title">
        <h2 id="current-title">Current setup</h2>
        <div className="current-card" aria-busy={statusLoading}>
          {statusLoading && (
            <p className="status-message" role="status">Reading current setup…</p>
          )}
          {!statusLoading && statusError && (
            <div className="status-error" role="alert">
              <div>
                <strong>Current setup is unavailable</strong>
                <p>{statusError.message}</p>
              </div>
              <button className="text-button" onClick={loadStatus} type="button">
                Try again
              </button>
            </div>
          )}
          {!statusLoading && !statusError && status && (
            <>
              <ProviderMark providerId={status.provider} />
              <div className="current-copy">
                {!status.knownProvider && (
                  <span className="unmanaged-label">External / unmanaged provider</span>
                )}
                <h3>{activeProvider?.name ?? status.provider}</h3>
                <code title={status.target}>{status.target}</code>
              </div>
              <span className="state-pill active-pill">
                <span className="state-dot" aria-hidden="true" />
                Active
              </span>
            </>
          )}
        </div>
      </section>

      <section className="providers-section" aria-labelledby="providers-title">
        <div className="section-heading">
          <h2 id="providers-title">Providers</h2>
          {!providersLoading && !providersError && (
            <span className="provider-count">{providers.length} available</span>
          )}
        </div>

        {providersLoading && (
          <p className="section-message" role="status">Loading providers…</p>
        )}
        {!providersLoading && providersError && (
          <div className="inventory-error" role="alert">
            <p>Providers could not be loaded. Your current setup is still shown above.</p>
            <button className="text-button" onClick={loadProviders} type="button">
              Try again
            </button>
          </div>
        )}
        {!providersLoading && !providersError && providers.length === 0 && (
          <p className="section-message">No providers are available.</p>
        )}
        {!providersLoading && !providersError && providers.length > 0 && (
          <ul className="provider-grid" aria-label="Choose a provider to inspect">
            {providers.map((provider) => {
              const isSelected = selectedProvider === provider.id;
              const isActive = status?.knownProvider && status.provider === provider.id;
              return (
                <li key={provider.id}>
                  <button
                    className={`provider-card${isSelected ? " is-selected" : ""}`}
                    type="button"
                    aria-pressed={isSelected}
                    onClick={() => handleProviderSelect(provider)}
                  >
                    <ProviderMark providerId={provider.id} />
                    <span className="provider-card-copy">
                      <strong>{provider.name}</strong>
                      <span>{transportLabel(provider.transport)}</span>
                    </span>
                    <span className="provider-card-states">
                      {isActive && <span className="state-pill active-pill">Active</span>}
                      {isSelected && <span className="state-pill selected-pill">Selected</span>}
                    </span>
                  </button>
                </li>
              );
            })}
          </ul>
        )}
      </section>

      <section className="details-section" aria-labelledby="details-title">
        <h2 id="details-title">Selected provider</h2>
        {selected ? (
          <ProviderDetails
            key={selected.id}
            provider={selected}
            credentialStatus={credentialStatus}
            credentialOperation={credentialOperation}
            credentialError={credentialError}
            models={visibleModels}
            modelsLoading={modelsOperation === selected.id}
            modelsError={modelsError}
            selectedModel={
              selectedModel?.providerId === selected.id ? selectedModel.modelId : ""
            }
            nativeModel={nativeModels[selected.id] ?? ""}
            switchPending={switchOperation !== null}
            switchError={switchError}
            successTarget={
              switchSuccess?.providerId === selected.id ? switchSuccess.target : null
            }
            onCheckCredential={() => void inspectCredential(selected.id)}
            onSaveCredential={(credential) => saveProviderCredential(selected.id, credential)}
            onLoadModels={() => void loadProviderModels(selected.id)}
            onSelectModel={(modelId) => setSelectedModel({
              providerId: selected.id,
              modelId,
            })}
            onNativeModelChange={(modelId) => setNativeModels((current) => ({
              ...current,
              [selected.id]: modelId,
            }))}
            onSwitch={(modelId) => void switchProviderModel(selected.id, modelId)}
          />
        ) : (
          <div className="details-empty">
            <p>
              {providersLoading
                ? "Choose a provider to see its details."
                : "Select a provider to see its details."}
            </p>
          </div>
        )}
      </section>
    </main>
  );
}

function asDesktopError(reason: unknown): DesktopError {
  if (
    typeof reason === "object" &&
    reason !== null &&
    "code" in reason &&
    typeof reason.code === "string" &&
    "message" in reason &&
    typeof reason.message === "string"
  ) {
    return { code: reason.code, message: reason.message };
  }
  return {
    code: "internal_error",
    message: "This operation could not be completed. Try again.",
  };
}

function transportLabel(transport: Provider["transport"]): string {
  switch (transport) {
    case "native":
      return "Native";
    case "responses":
      return "Direct API";
    case "bridge":
      return "Connected service";
  }
}
