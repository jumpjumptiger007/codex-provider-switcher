import openAiMark from "../assets/providers/openai-blossom.svg";
import lmStudioMark from "../assets/providers/lm-studio-icon-color.svg";
import openRouterMark from "../assets/providers/openrouter-glyph-ink.svg";

const PROVIDER_INITIALS: Record<string, string> = {
  deepseek: "DS",
  lmstudio: "LM",
  ollama: "OL",
  openai: "OA",
  openrouter: "OR",
  xai: "XA",
};

const PROVIDER_MARKS: Record<string, string> = {
  lmstudio: lmStudioMark,
  openai: openAiMark,
  openrouter: openRouterMark,
};

interface ProviderMarkProps {
  providerId: string;
}

export default function ProviderMark({ providerId }: ProviderMarkProps) {
  const initials = PROVIDER_INITIALS[providerId] ?? providerId.slice(0, 2).toUpperCase();
  const className = `provider-mark provider-mark--${providerId.replace(/[^a-z0-9-]/gi, "")}`;
  const mark = PROVIDER_MARKS[providerId];

  return (
    <span className={`${className}${mark ? " provider-mark--logo" : ""}`} aria-hidden="true">
      {mark ? <img className="provider-mark-logo" src={mark} alt="" /> : initials}
    </span>
  );
}
