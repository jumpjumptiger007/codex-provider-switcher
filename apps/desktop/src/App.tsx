import { useEffect, useState } from "react";
import { getProviders } from "./lib/cps";
import type { Provider } from "./types/provider";

export default function App() {
  const [providers, setProviders] = useState<Provider[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    let active = true;

    getProviders()
      .then((inventory) => {
        if (active) setProviders(inventory);
      })
      .catch((reason: unknown) => {
        if (active) {
          setError(reason instanceof Error ? reason.message : String(reason));
        }
      })
      .finally(() => {
        if (active) setLoading(false);
      });

    return () => {
      active = false;
    };
  }, []);

  return (
    <main className="page" aria-busy={loading}>
      <header className="page-header">
        <p className="eyebrow">Desktop frontend</p>
        <h1>Codex Provider Switcher</h1>
        <p className="subtitle">Available providers from the CPS core registry.</p>
      </header>

      <section className="inventory" aria-labelledby="inventory-title">
        <div className="section-heading">
          <h2 id="inventory-title">Provider inventory</h2>
          {!loading && !error && (
            <span className="count">{providers.length} providers</span>
          )}
        </div>

        {loading && <p className="message">Loading providers…</p>}
        {error && (
          <p className="message error" role="alert">
            Could not load providers: {error}
          </p>
        )}
        {!loading && !error && (
          <ul className="provider-list">
            {providers.map((provider) => (
              <li className="provider-row" key={provider.id}>
                <div className="provider-name">
                  <strong>{provider.name}</strong>
                  <code>{provider.id}</code>
                </div>
                <div className="provider-details">
                  <span>{provider.transport}</span>
                  <span>{provider.compatibility}</span>
                </div>
              </li>
            ))}
          </ul>
        )}
      </section>
    </main>
  );
}
