const PROVIDER_INITIALS: Record<string, string> = {
  deepseek: "DS",
  lmstudio: "LM",
  ollama: "OL",
  openai: "OA",
  openrouter: "OR",
  xai: "XA",
};

interface ProviderMarkProps {
  providerId: string;
}

export default function ProviderMark({ providerId }: ProviderMarkProps) {
  const initials = PROVIDER_INITIALS[providerId] ?? providerId.slice(0, 2).toUpperCase();
  const className = `provider-mark provider-mark--${providerId.replace(/[^a-z0-9-]/gi, "")}`;

  return (
    <span className={className} aria-hidden="true">
      {initials}
    </span>
  );
}
