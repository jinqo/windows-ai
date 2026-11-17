/** Ready-made settings for free or local OpenAI-compatible endpoints. */
export interface ProviderPreset {
  id: string;
  label: string;
  base_url: string;
  model: string;
  use_api_key: boolean;
  /** Where to get a (free) key; empty for local servers. */
  key_url: string;
  note: string;
}

export const PROVIDER_PRESETS: ProviderPreset[] = [
  {
    id: "ollama",
    label: "Ollama (local, free)",
    base_url: "http://localhost:11434/v1",
    model: "llama3.2",
    use_api_key: false,
    key_url: "https://ollama.com/download",
    note: "Runs on your PC, no account. Install Ollama, then `ollama pull llama3.2`.",
  },
  {
    id: "lmstudio",
    label: "LM Studio (local, free)",
    base_url: "http://localhost:1234/v1",
    model: "",
    use_api_key: false,
    key_url: "https://lmstudio.ai",
    note: "Runs on your PC. Start the local server in LM Studio, then press Load models.",
  },
  {
    id: "groq",
    label: "Groq (free tier)",
    base_url: "https://api.groq.com/openai/v1",
    model: "llama-3.3-70b-versatile",
    use_api_key: true,
    key_url: "https://console.groq.com/keys",
    note: "Free API key with rate limits. Very fast.",
  },
  {
    id: "gemini",
    label: "Google Gemini (free tier)",
    base_url: "https://generativelanguage.googleapis.com/v1beta/openai",
    model: "gemini-2.5-flash",
    use_api_key: true,
    key_url: "https://aistudio.google.com/apikey",
    note: "Free API key from Google AI Studio, with daily limits.",
  },
  {
    id: "openrouter",
    label: "OpenRouter (free models)",
    base_url: "https://openrouter.ai/api/v1",
    model: "meta-llama/llama-3.3-70b-instruct:free",
    use_api_key: true,
    key_url: "https://openrouter.ai/keys",
    note: "Free key; models ending in :free cost nothing (rate limited). Press Load models to see them.",
  },
];

/** Finds the preset matching a base URL, if any. */
export function presetForUrl(baseUrl: string): ProviderPreset | undefined {
  const norm = (u: string) => u.trim().replace(/\/+$/, "").toLowerCase();
  return PROVIDER_PRESETS.find((p) => norm(p.base_url) === norm(baseUrl));
}

/** Free models first (ids ending in ":free"), then alphabetical. */
export function sortModels<T extends { id: string }>(models: T[]): T[] {
  return [...models].sort((a, b) => {
    const fa = a.id.endsWith(":free") ? 0 : 1;
    const fb = b.id.endsWith(":free") ? 0 : 1;
    return fa - fb || a.id.localeCompare(b.id);
  });
}
