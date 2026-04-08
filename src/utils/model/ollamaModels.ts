/**
 * Ollama model discovery for the /model picker.
 * Fetches available models from the Ollama API and caches them
 * so the synchronous getModelOptions() can use them.
 *
 * Supports both local and cloud models (:cloud suffix).
 * Cloud models require 'ollama signin' authentication.
 */

import type { ModelOption } from './modelOptions.js'
import { getOllamaApiBaseUrl, isCloudModel } from '../providerDiscovery.js'

let cachedOllamaOptions: ModelOption[] | null = null
let cachedAt = 0
const CACHE_TTL_MS = 5 * 60 * 1000 // 5 minutes
let fetchPromise: Promise<ModelOption[]> | null = null

/**
 * Known Ollama cloud models. These always appear in the /model picker
 * so users can discover and select them even before pulling.
 */
const KNOWN_CLOUD_MODELS: Array<{ value: string; label: string; description: string }> = [
  { value: 'qwen3.5:397b-cloud', label: '☁️ qwen3.5:397b-cloud', description: 'Qwen 3.5 Coder 480B' },
  { value: 'qwen3.5:cloud', label: '☁️ qwen3.5:cloud', description: 'Qwen 3.5' },
  { value: 'deepseek-v3.2:cloud', label: '☁️ deepseek-v3.2:cloud', description: 'DeepSeek V3.2' },
  { value: 'deepseek-v3:cloud', label: '☁️ deepseek-v3:cloud', description: 'DeepSeek V3' },
  { value: 'glm-5:cloud', label: '☁️ glm-5:cloud', description: 'GLM-5' },
  { value: 'kimi-k2.5:cloud', label: '☁️ kimi-k2.5:cloud', description: 'Kimi K2.5' },
  { value: 'minimax-m2.5:cloud', label: '☁️ minimax-m2.5:cloud', description: 'MiniMax M2.5' },
  { value: 'minimax-m2.7:cloud', label: '☁️ minimax-m2.7:cloud', description: 'MiniMax M2.7' },
  { value: 'nemotron-3-super:cloud', label: '☁️ nemotron-3-super:cloud', description: 'Nemotron 3 Super' },
]

/**
 * Returns true when the current OPENAI_BASE_URL points at an Ollama instance.
 * Detects OLLAMA_BASE_URL presence, /v1 suffixed URLs, and the raw base URL.
 */
export function isOllamaProvider(): boolean {
  // Explicit OLLAMA_BASE_URL is always sufficient
  if (process.env.OLLAMA_BASE_URL) return true
  if (!process.env.OPENAI_BASE_URL) return false
  const baseUrl = process.env.OPENAI_BASE_URL
  // Match common Ollama port
  try {
    const parsed = new URL(baseUrl)
    if (parsed.port === '11434') return true
  } catch {
    // ignore
  }
  return false
}

/**
 * Fetch models from the Ollama /api/tags endpoint.
 */
export async function fetchOllamaModels(): Promise<ModelOption[]> {
  const apiUrl = getOllamaApiBaseUrl()
  if (!apiUrl) return []

  const controller = new AbortController()
  const timeout = setTimeout(() => controller.abort(), 5000)

  try {
    const response = await fetch(`${apiUrl}/api/tags`, {
      method: 'GET',
      signal: controller.signal,
    })
    if (!response.ok) return []

    const data = (await response.json()) as {
      models?: Array<{
        name?: string
        size?: number
        details?: {
          parameter_size?: string
          quantization_level?: string
          family?: string
        }
      }>
    }

    return (data.models ?? [])
      .filter(m => Boolean(m.name))
      .map(m => {
        const paramSize = m.details?.parameter_size ?? ''
        const quant = m.details?.quantization_level ?? ''
        const sizeGB = m.size ? `${(m.size / 1e9).toFixed(1)}GB` : ''
        const isCloud = isCloudModel(m.name!)
        const cloudLabel = isCloud ? '☁️ ' : ''
        const localParts = [paramSize, quant, sizeGB].filter(Boolean).join(' · ')
        const description = isCloud
          ? 'Ollama cloud model'
          : (localParts ? `Ollama · ${localParts}` : 'Ollama model')
        return {
          value: m.name!,
          label: `${cloudLabel}${m.name!}`,
          description,
        }
      })
  } catch {
    return []
  } finally {
    clearTimeout(timeout)
  }
}

/**
 * Prefetch and cache Ollama models. Call during startup.
 */
export function prefetchOllamaModels(): void {
  if (!isOllamaProvider()) return
  const cacheValid = cachedOllamaOptions && cachedOllamaOptions.length > 0 && (Date.now() - cachedAt < CACHE_TTL_MS)
  if (cacheValid) return
  if (fetchPromise) return
  fetchPromise = fetchOllamaModels()
    .then(options => {
      cachedOllamaOptions = options
      cachedAt = Date.now()
      return options
    })
    .finally(() => {
      fetchPromise = null
    })
}

/**
 * Get cached Ollama model options (synchronous).
 * Merges fetched local models with known cloud models so cloud models
 * always appear in the /model picker even if not yet pulled.
 */
export function getCachedOllamaModelOptions(): ModelOption[] {
  const fetched = cachedOllamaOptions ?? []
  const fetchedValues = new Set(fetched.map(m => m.value))
  // Append known cloud models that aren't already in the fetched list
  const missing = KNOWN_CLOUD_MODELS.filter(m => !fetchedValues.has(m.value))
  return [...fetched, ...missing]
}

/**
 * Pull an Ollama model if it's not already available locally.
 * Runs `ollama pull <model>` and streams progress to a callback.
 * Returns true on success, false on failure.
 */
export async function ensureOllamaModelPulled(
  modelName: string,
  onProgress?: (message: string) => void,
): Promise<boolean> {
  const apiUrl = getOllamaApiBaseUrl()
  if (!apiUrl) return false

  // Check if model is already available
  const cached = cachedOllamaOptions ?? []
  if (cached.some(m => m.value === modelName)) return true

  onProgress?.(`Pulling ${modelName}...`)

  const controller = new AbortController()
  const timeout = setTimeout(() => controller.abort(), 300_000) // 5 min timeout for large models

  try {
    const response = await fetch(`${apiUrl}/api/pull`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ name: modelName, stream: true }),
      signal: controller.signal,
    })
    if (!response.ok) {
      const body = await response.text().catch(() => '')
      onProgress?.(`Failed to pull ${modelName}: ${response.status} ${body}`)
      return false
    }

    // Stream the pull progress
    const reader = response.body?.getReader()
    if (!reader) return false
    const decoder = new TextDecoder()
    let lastStatus = ''

    while (true) {
      const { done, value } = await reader.read()
      if (done) break
      const chunk = decoder.decode(value, { stream: true })
      for (const line of chunk.split('\n').filter(Boolean)) {
        try {
          const json = JSON.parse(line) as { status?: string; completed?: number; total?: number }
          if (json.status && json.status !== lastStatus) {
            lastStatus = json.status
            if (json.total && json.completed) {
              const pct = Math.round((json.completed / json.total) * 100)
              onProgress?.(`${json.status} ${pct}%`)
            } else {
              onProgress?.(json.status)
            }
          }
        } catch {
          // ignore malformed JSON lines
        }
      }
    }

    // Invalidate cache so next fetch picks up the new model
    cachedOllamaOptions = null
    cachedAt = 0

    onProgress?.(`${modelName} is ready`)
    return true
  } catch (e) {
    onProgress?.(`Failed to pull ${modelName}: ${(e as Error).message}`)
    return false
  } finally {
    clearTimeout(timeout)
  }
}
