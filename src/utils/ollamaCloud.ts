/**
 * Ollama Cloud provider utilities.
 *
 * Ollama Cloud models run on ollama.com's infrastructure and are accessed
 * via the OpenAI-compatible API. They don't support Anthropic's prompt cache,
 * so we optimize for parallel processing instead.
 *
 * Uses ollama.com/api/show endpoint to discover model metadata dynamically.
 */

import { isEnvTruthy } from './envUtils.js'

/**
 * In-memory cache for discovered model configs.
 */
const discoveredModelCache = new Map<
  string,
  { config: OllamaCloudConfig; discoveredAt: number }
>()

const CACHE_TTL_MS = 10 * 60 * 1000 // 10 minutes

/**
 * Discover model configuration from Ollama Cloud /api/show endpoint.
 */
async function discoverModelConfig(model: string): Promise<OllamaCloudConfig | null> {
  try {
    const response = await fetch('https://ollama.com/api/show', {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
      },
      body: JSON.stringify({ model }),
    })

    if (!response.ok) {
      return null
    }

    const data = await response.json()

    // Extract context length from model_info
    // Keys are like "glm5.1.context_length", "qwen3moe.context_length", etc.
    let contextWindow: number | undefined
    for (const [key, value] of Object.entries(data.model_info || {})) {
      if (key.endsWith('.context_length') && typeof value === 'number') {
        contextWindow = value
        break
      }
    }

    if (!contextWindow) {
      return null
    }

    // Derive reasonable defaults based on context window
    const maxOutputTokens = Math.min(16_384, Math.floor(contextWindow * 0.08))
    const chunkSize = Math.min(40_000, Math.floor(contextWindow * 0.15))
    const mergeMaxOutput = Math.min(24_576, Math.floor(maxOutputTokens * 1.5))

    return {
      contextWindow,
      maxOutputTokens,
      chunkSize,
      mergeMaxOutput,
    }
  } catch (error) {
    console.debug(`[ollama-cloud] Failed to discover model ${model}:`, error)
    return null
  }
}

/**
 * Detect if using an Ollama Cloud model.
 * Cloud models have the :cloud or -cloud suffix (e.g., glm-5:cloud, qwen3.5:397b-cloud)
 */
export function isOllamaCloudProvider(model: string): boolean {
  return model.includes(':cloud') || model.includes('-cloud')
}

/**
 * Detect if using any Ollama provider (local or cloud).
 */
export function isOllamaProvider(model: string): boolean {
  return isOllamaCloudProvider(model) || isLocalOllamaProvider()
}

/**
 * Detect local Ollama provider via environment.
 */
export function isLocalOllamaProvider(): boolean {
  if (process.env.OLLAMA_BASE_URL) return true
  if (!isEnvTruthy(process.env.CLAUDE_CODE_USE_OPENAI)) return false
  if (!process.env.OPENAI_BASE_URL) return false

  try {
    const parsed = new URL(process.env.OPENAI_BASE_URL)
    // Common Ollama ports
    return parsed.port === '11434' || parsed.hostname === 'localhost' || parsed.hostname === '127.0.0.1'
  } catch {
    return false
  }
}

/**
 * Configuration for Ollama Cloud models.
 *
 * Key design decisions:
 * - chunkSize: Small enough for fast individual processing, large enough to minimize chunks
 * - maxOutput: Conservative limits keep summaries concise and reduce merge complexity
 * - contextWindow: Matches openaiContextWindows.ts for consistency
 */
export interface OllamaCloudConfig {
  contextWindow: number
  maxOutputTokens: number
  chunkSize: number
  mergeMaxOutput: number
}

const OLLAMA_CLOUD_CONFIGS: Record<string, OllamaCloudConfig> = {
  // Qwen 3.5 Coder 480B - largest model, can handle detailed summaries
  'qwen3.5:397b-cloud': {
    contextWindow: 256_000,
    maxOutputTokens: 16_384,
    chunkSize: 40_000,
    mergeMaxOutput: 24_576,
  },
  // Qwen 3.5 - general purpose
  'qwen3.5:cloud': {
    contextWindow: 256_000,
    maxOutputTokens: 16_384,
    chunkSize: 40_000,
    mergeMaxOutput: 24_576,
  },
  // DeepSeek V3.2 - newer version
  'deepseek-v3.2:cloud': {
    contextWindow: 256_000,
    maxOutputTokens: 16_384,
    chunkSize: 40_000,
    mergeMaxOutput: 24_576,
  },
  // DeepSeek V3 - original version
  'deepseek-v3:cloud': {
    contextWindow: 256_000,
    maxOutputTokens: 16_384,
    chunkSize: 40_000,
    mergeMaxOutput: 24_576,
  },
  // GLM-5
  'glm-5:cloud': {
    contextWindow: 256_000,
    maxOutputTokens: 16_384,
    chunkSize: 40_000,
    mergeMaxOutput: 24_576,
  },
  // GLM-5.1 (newer version)
  'glm-5.1:cloud': {
    contextWindow: 256_000,
    maxOutputTokens: 16_384,
    chunkSize: 40_000,
    mergeMaxOutput: 24_576,
  },
  // Kimi K2.5
  'kimi-k2.5:cloud': {
    contextWindow: 256_000,
    maxOutputTokens: 16_384,
    chunkSize: 40_000,
    mergeMaxOutput: 24_576,
  },
  // MiniMax models
  'minimax-m2.5:cloud': {
    contextWindow: 256_000,
    maxOutputTokens: 16_384,
    chunkSize: 40_000,
    mergeMaxOutput: 24_576,
  },
  'minimax-m2.7:cloud': {
    contextWindow: 256_000,
    maxOutputTokens: 16_384,
    chunkSize: 40_000,
    mergeMaxOutput: 24_576,
  },
  // Nemotron
  'nemotron-3-super:cloud': {
    contextWindow: 256_000,
    maxOutputTokens: 16_384,
    chunkSize: 40_000,
    mergeMaxOutput: 24_576,
  },
  // Default for unknown cloud models - conservative settings
  'default': {
    contextWindow: 256_000,
    maxOutputTokens: 8_192,
    chunkSize: 30_000,
    mergeMaxOutput: 16_384,
  },
}

/**
 * Get configuration for an Ollama Cloud model.
 * Uses a hybrid approach:
 * 1. Check static config (fast path for known models)
 * 2. Check cache (for previously discovered models)
 * 3. Smart prefix matching on static config
 * 4. API discovery via ollama.com/api/show
 * 5. Fall back to conservative defaults
 */
export async function getOllamaCloudConfig(model: string): Promise<OllamaCloudConfig> {
  // 1. Exact match - fast path
  if (OLLAMA_CLOUD_CONFIGS[model]) {
    return OLLAMA_CLOUD_CONFIGS[model]
  }

  // 2. Cache check
  const cached = discoveredModelCache.get(model)
  if (cached && Date.now() - cached.discoveredAt < CACHE_TTL_MS) {
    return cached.config
  }

  // 3. Smart prefix matching on static config
  const cloudSuffix = model.includes('-cloud') ? '-cloud' : model.includes(':cloud') ? ':cloud' : null
  if (cloudSuffix) {
    const modelPrefix = model.split(cloudSuffix)[0]
    const sortedKeys = Object.keys(OLLAMA_CLOUD_CONFIGS)
      .filter(k => k !== 'default')
      .sort((a, b) => b.length - a.length)

    for (const key of sortedKeys) {
      const keyPrefix = key.split(/[-:]cloud/)[0]
      if (modelPrefix === keyPrefix || modelPrefix.startsWith(keyPrefix + '.') || keyPrefix.startsWith(modelPrefix + '.')) {
        return OLLAMA_CLOUD_CONFIGS[key]
      }
    }
  }

  // 4. API discovery
  const discovered = await discoverModelConfig(model)
  if (discovered) {
    discoveredModelCache.set(model, {
      config: discovered,
      discoveredAt: Date.now(),
    })
    return discovered
  }

  // 5. Conservative defaults
  return OLLAMA_CLOUD_CONFIGS['default']
}

/**
 * Check if parallel compaction should be used.
 * Only worthwhile for large conversations that will split into multiple chunks.
 */
export async function shouldUseParallelCompact(model: string, tokenCount: number): Promise<boolean> {
  if (!isOllamaCloudProvider(model)) {
    return false
  }

  const config = await getOllamaCloudConfig(model)
  // Need at least 2 chunks for parallelization to make sense
  const minTokensForParallel = config.chunkSize * 2

  return tokenCount >= minTokensForParallel
}
