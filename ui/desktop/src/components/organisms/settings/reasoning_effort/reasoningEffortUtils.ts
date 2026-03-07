/**
 * Check if the current model supports reasoning effort configuration.
 *
 * Supported models:
 * - OpenAI o-series (o1, o1-pro, o3, o3-mini, o4-mini) and gpt-5
 * - Anthropic Claude models (extended thinking via budget_tokens)
 * - Google Gemini thinking models
 * - DeepSeek reasoning models
 *
 * Returns true if reasoning effort is supported, false otherwise.
 */
export function supportsReasoningEffort(
  modelName: string | null,
  providerName: string | null
): boolean {
  if (!modelName) return false;

  const model = modelName.toLowerCase();
  const provider = (providerName || '').toLowerCase();

  // OpenAI o-series and gpt-5
  if (
    model.startsWith('o1') ||
    model.startsWith('o3') ||
    model.startsWith('o4') ||
    model.startsWith('gpt-5')
  ) {
    return true;
  }

  // Anthropic Claude models (extended thinking)
  if (provider.includes('anthropic') || model.includes('claude')) {
    return true;
  }

  // Google Gemini thinking models
  if (model.includes('gemini') && model.includes('thinking')) {
    return true;
  }

  // DeepSeek reasoning models
  if (model.includes('deepseek') && (model.includes('r1') || model.includes('reasoner'))) {
    return true;
  }

  // For unknown providers/models, allow it — the server will handle gracefully
  // Only explicitly disable for models we KNOW don't support it
  if (
    model.startsWith('gpt-4') ||
    model.startsWith('gpt-3') ||
    model.includes('llama') ||
    model.includes('mistral') ||
    model.includes('mixtral') ||
    model.includes('phi-') ||
    model.includes('qwen')
  ) {
    return false;
  }

  // Default: allow — better to show the option than hide it incorrectly
  return true;
}
