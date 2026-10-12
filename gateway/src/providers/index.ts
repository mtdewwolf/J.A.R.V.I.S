// Provider registry. Provider identity is configuration, not architecture:
// the core never references provider-specific shapes (ADR-0004/0005).

import type { InferenceProvider } from "../types";
import { AnthropicProvider } from "./anthropic";
import { FakeProvider } from "./fake";
import { OllamaProvider } from "./ollama";

export type ProviderSource = "fake" | "anthropic" | "ollama";

export function buildProviders(env: Record<string, string | undefined>): Map<string, InferenceProvider> {
  const providers = new Map<string, InferenceProvider>();
  providers.set("fake", new FakeProvider());
  if (env.ANTHROPIC_API_KEY) {
    providers.set("anthropic", new AnthropicProvider(env.ANTHROPIC_API_KEY));
  }
  if (env.JARVIS_ENABLE_OLLAMA === "1") {
    providers.set("ollama", new OllamaProvider(env.OLLAMA_HOST));
  }
  return providers;
}
