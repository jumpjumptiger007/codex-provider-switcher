import { useCallback, useEffect, useRef, useState } from "react";
import { getProviders, getStatus } from "./lib/cps";
import type { DesktopError } from "./lib/cps";
import ProviderMark from "./components/ProviderMark";
import type { Provider } from "./types/provider";
import type { Status } from "./types/provider";

export default function App() {
  const [providers, setProviders] = useState<Provider[]>([]);
  const [providersLoading, setProvidersLoading] = useState(true);
  const [providersError, setProvidersError] = useState(false);
  const [status, setStatus] = useState<Status | null>(null);
  const [statusLoading, setStatusLoading] = useState(true);
  const [statusError, setStatusError] = useState<DesktopError | null>(null);
  const [selectedProvider, setSelectedProvider] = useState<string | null>(null);
  const selectedByUser = useRef(false);
  const providerSnapshot = useRef<Provider[] | null>(null);
  const statusSnapshot = useRef<Status | null>(null);

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
          setSelectedProvider(activeProvider?.id ?? inventory[0]?.id ?? null);
        }
      })
      .catch(() => setProvidersError(true))
      .finally(() => setProvidersLoading(false));
  }, []);

  const loadStatus = useCallback(() => {
    setStatusLoading(true);
    setStatusError(null);
    getStatus()
      .then((current) => {
        statusSnapshot.current = current;
        setStatus(current);
        if (!selectedByUser.current) {
          const activeProvider = current.knownProvider
            ? providerSnapshot.current?.find(
                (provider) => provider.id === current.provider,
              )
            : undefined;
          setSelectedProvider(
            activeProvider?.id ?? providerSnapshot.current?.[0]?.id ?? null,
          );
        }
      })
      .catch((reason: DesktopError) => setStatusError(reason))
      .finally(() => setStatusLoading(false));
  }, []);

  useEffect(() => {
    loadProviders();
    loadStatus();
  }, [loadProviders, loadStatus]);

  const selected = providers.find((provider) => provider.id === selectedProvider);
  const activeProvider = status?.knownProvider
    ? providers.find((provider) => provider.id === status.provider)
    : undefined;

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
            <p className="status-message" role="status">
              Reading current setup…
            </p>
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
                  <span className="unmanaged-label">
                    External / unmanaged provider
                  </span>
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
            <span className="provider-count">
              {providers.length} available
            </span>
          )}
        </div>

        {providersLoading && (
          <p className="section-message" role="status">
            Loading providers…
          </p>
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
                    onClick={() => {
                      selectedByUser.current = true;
                      setSelectedProvider(provider.id);
                    }}
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
          <article className="details-card" aria-live="polite">
            <div className="details-header">
              <ProviderMark providerId={selected.id} />
              <div>
                <h3>{selected.name}</h3>
                <p>{transportDescription(selected.transport)}</p>
              </div>
            </div>
            <dl className="detail-list">
              <div>
                <dt>Models</dt>
                <dd>{modelDescription(selected.discovery)}</dd>
              </div>
              <div>
                <dt>Credential</dt>
                <dd>{selected.requiresCredential ? "Required" : "Managed outside CPS"}</dd>
              </div>
              <div>
                <dt>Compatibility</dt>
                <dd>{compatibilityLabel(selected.compatibility)}</dd>
              </div>
            </dl>
          </article>
        ) : (
          <div className="details-empty">
            <p>{providersLoading ? "Choose a provider to see its details." : "Select a provider to see its details."}</p>
          </div>
        )}
      </section>
    </main>
  );
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

function transportDescription(transport: Provider["transport"]): string {
  return transportLabel(transport) + " provider";
}

function modelDescription(discovery: Provider["discovery"]): string {
  return discovery === "codex-managed" ? "Managed by Codex" : "Available from provider";
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
