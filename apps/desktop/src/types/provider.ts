export type ProviderTransport = "native" | "responses" | "bridge";

export type ProviderCompatibility =
  | "verified"
  | "verified-basic"
  | "degraded"
  | "unverified"
  | "unsupported";

export type ProviderDiscovery = "codex-managed" | "provider-models";

export interface Provider {
  id: string;
  name: string;
  transport: ProviderTransport;
  compatibility: ProviderCompatibility;
  discovery: ProviderDiscovery;
  requiresCredential: boolean;
}

export interface Status {
  provider: string;
  model: string;
  target: string;
  knownProvider: boolean;
  transport: ProviderTransport | null;
}
