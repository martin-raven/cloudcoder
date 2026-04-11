import { afterEach, beforeEach, describe, expect, test } from 'bun:test'

import {
  isOllamaCloudProvider,
  isOllamaProvider,
  isLocalOllamaProvider,
  getOllamaCloudConfig,
  shouldUseParallelCompact,
} from './ollamaCloud.ts'

const originalEnv = {
  OLLAMA_BASE_URL: process.env.OLLAMA_BASE_URL,
  CLAUDE_CODE_USE_OPENAI: process.env.CLAUDE_CODE_USE_OPENAI,
  OPENAI_BASE_URL: process.env.OPENAI_BASE_URL,
}

function restoreEnv(): void {
  for (const [key, value] of Object.entries(originalEnv)) {
    if (value === undefined) {
      delete process.env[key as keyof typeof process.env]
    } else {
      process.env[key as keyof typeof process.env] = value
    }
  }
}

beforeEach(() => {
  restoreEnv()
})

afterEach(() => {
  restoreEnv()
})

describe('isOllamaCloudProvider', () => {
  test('returns true for models with :cloud suffix', () => {
    expect(isOllamaCloudProvider('glm-5:cloud')).toBe(true)
    expect(isOllamaCloudProvider('glm-5.1:cloud')).toBe(true)
    expect(isOllamaCloudProvider('deepseek-v3:cloud')).toBe(true)
    expect(isOllamaCloudProvider('kimi-k2.5:cloud')).toBe(true)
  })

  test('returns true for models with -cloud suffix', () => {
    expect(isOllamaCloudProvider('qwen3.5:397b-cloud')).toBe(true)
    expect(isOllamaCloudProvider('qwen3.5:cloud')).toBe(true)
    expect(isOllamaCloudProvider('deepseek-v3.2:cloud')).toBe(true)
    expect(isOllamaCloudProvider('nemotron-3-super:cloud')).toBe(true)
  })

  test('returns false for models without cloud suffix', () => {
    expect(isOllamaCloudProvider('glm-5')).toBe(false)
    expect(isOllamaCloudProvider('llama3.2:3b')).toBe(false)
    expect(isOllamaCloudProvider('qwen2.5-coder:32b')).toBe(false)
    expect(isOllamaCloudProvider('gpt-4o')).toBe(false)
  })

  test('returns false for empty or undefined model names', () => {
    expect(isOllamaCloudProvider('')).toBe(false)
  })
})

describe('isOllamaProvider', () => {
  test('returns true for cloud models', () => {
    expect(isOllamaProvider('glm-5:cloud')).toBe(true)
    expect(isOllamaProvider('qwen3.5:cloud')).toBe(true)
  })

  test('returns true for local ollama when OLLAMA_BASE_URL is set', () => {
    process.env.OLLAMA_BASE_URL = 'http://localhost:11434'
    expect(isOllamaProvider('llama3.2:3b')).toBe(true)
  })

  test('returns true for local ollama via OPENAI_BASE_URL on localhost', () => {
    process.env.CLAUDE_CODE_USE_OPENAI = '1'
    process.env.OPENAI_BASE_URL = 'http://localhost:11434'
    expect(isOllamaProvider('llama3.2:3b')).toBe(true)
  })

  test('returns true for local ollama via OPENAI_BASE_URL on 127.0.0.1', () => {
    process.env.CLAUDE_CODE_USE_OPENAI = '1'
    process.env.OPENAI_BASE_URL = 'http://127.0.0.1:11434'
    expect(isOllamaProvider('llama3.2:3b')).toBe(true)
  })

  test('returns false for non-ollama providers', () => {
    // Ensure no ollama env vars are set
    delete process.env.OLLAMA_BASE_URL
    delete process.env.CLAUDE_CODE_USE_OPENAI
    delete process.env.OPENAI_BASE_URL

    expect(isOllamaProvider('gpt-4o')).toBe(false)
    expect(isOllamaProvider('claude-sonnet-4-5')).toBe(false)
  })
})

describe('isLocalOllamaProvider', () => {
  test('returns true when OLLAMA_BASE_URL is set', () => {
    process.env.OLLAMA_BASE_URL = 'http://localhost:11434'
    expect(isLocalOllamaProvider()).toBe(true)

    process.env.OLLAMA_BASE_URL = 'http://192.168.1.100:11434'
    expect(isLocalOllamaProvider()).toBe(true)
  })

  test('returns true when OPENAI_BASE_URL points to localhost:11434', () => {
    process.env.CLAUDE_CODE_USE_OPENAI = '1'
    process.env.OPENAI_BASE_URL = 'http://localhost:11434'
    expect(isLocalOllamaProvider()).toBe(true)
  })

  test('returns true when OPENAI_BASE_URL points to 127.0.0.1:11434', () => {
    process.env.CLAUDE_CODE_USE_OPENAI = '1'
    process.env.OPENAI_BASE_URL = 'http://127.0.0.1:11434'
    expect(isLocalOllamaProvider()).toBe(true)
  })

  test('returns false when CLAUDE_CODE_USE_OPENAI is not set', () => {
    delete process.env.CLAUDE_CODE_USE_OPENAI
    process.env.OPENAI_BASE_URL = 'http://localhost:11434'
    expect(isLocalOllamaProvider()).toBe(false)
  })

  test('returns false when OPENAI_BASE_URL is not localhost', () => {
    process.env.CLAUDE_CODE_USE_OPENAI = '1'
    process.env.OPENAI_BASE_URL = 'https://api.openai.com/v1'
    expect(isLocalOllamaProvider()).toBe(false)
  })

  test('returns true when OPENAI_BASE_URL is localhost on any port', () => {
    // The function checks for localhost hostname, not specific port
    process.env.CLAUDE_CODE_USE_OPENAI = '1'
    process.env.OPENAI_BASE_URL = 'http://localhost:8080'
    expect(isLocalOllamaProvider()).toBe(true)
  })

  test('returns false when OPENAI_BASE_URL is not localhost', () => {
    process.env.CLAUDE_CODE_USE_OPENAI = '1'
    process.env.OPENAI_BASE_URL = 'http://192.168.1.100:8080'
    expect(isLocalOllamaProvider()).toBe(false)
  })

  test('returns false when no ollama indicators are present', () => {
    delete process.env.OLLAMA_BASE_URL
    delete process.env.CLAUDE_CODE_USE_OPENAI
    delete process.env.OPENAI_BASE_URL
    expect(isLocalOllamaProvider()).toBe(false)
  })
})

describe('getOllamaCloudConfig', () => {
  test('returns exact match for known models', async () => {
    const config = await getOllamaCloudConfig('glm-5:cloud')
    expect(config.contextWindow).toBe(256_000)
    expect(config.maxOutputTokens).toBe(16_384)
    expect(config.chunkSize).toBe(40_000)
    expect(config.mergeMaxOutput).toBe(24_576)
  })

  test('returns config for glm-5.1:cloud', async () => {
    const config = await getOllamaCloudConfig('glm-5.1:cloud')
    expect(config.contextWindow).toBe(256_000)
    expect(config.maxOutputTokens).toBe(16_384)
    expect(config.chunkSize).toBe(40_000)
    expect(config.mergeMaxOutput).toBe(24_576)
  })

  test('returns config for qwen3.5:397b-cloud', async () => {
    const config = await getOllamaCloudConfig('qwen3.5:397b-cloud')
    expect(config.contextWindow).toBe(256_000)
    expect(config.maxOutputTokens).toBe(16_384)
    expect(config.chunkSize).toBe(40_000)
    expect(config.mergeMaxOutput).toBe(24_576)
  })

  test('returns config for deepseek models', async () => {
    const config1 = await getOllamaCloudConfig('deepseek-v3:cloud')
    expect(config1.contextWindow).toBe(256_000)
    expect(config1.chunkSize).toBe(40_000)

    const config2 = await getOllamaCloudConfig('deepseek-v3.2:cloud')
    expect(config2.contextWindow).toBe(256_000)
    expect(config2.chunkSize).toBe(40_000)
  })

  test('returns config for kimi model', async () => {
    const config = await getOllamaCloudConfig('kimi-k2.5:cloud')
    expect(config.contextWindow).toBe(256_000)
    expect(config.maxOutputTokens).toBe(16_384)
  })

  test('returns config for MiniMax models', async () => {
    const config1 = await getOllamaCloudConfig('minimax-m2.5:cloud')
    expect(config1.contextWindow).toBe(256_000)

    const config2 = await getOllamaCloudConfig('minimax-m2.7:cloud')
    expect(config2.contextWindow).toBe(256_000)
  })

  test('returns config for nemotron model', async () => {
    const config = await getOllamaCloudConfig('nemotron-3-super:cloud')
    expect(config.contextWindow).toBe(256_000)
    expect(config.maxOutputTokens).toBe(16_384)
  })

  test('uses API discovery for cloud models', async () => {
    // Test that API discovery works - glm-5.99:cloud doesn't exist in static config
    // but should be discovered via API (or fall back to prefix matching)
    const config = await getOllamaCloudConfig('glm-5.99:cloud')
    expect(config.contextWindow).toBeGreaterThan(0)
    expect(config.chunkSize).toBeGreaterThan(0)
  })

  test('falls back to default for completely unknown models', async () => {
    const config = await getOllamaCloudConfig('unknown-model:cloud')
    expect(config.contextWindow).toBe(256_000)
    expect(config.maxOutputTokens).toBe(8_192) // Default is more conservative
    expect(config.chunkSize).toBe(30_000)
    expect(config.mergeMaxOutput).toBe(16_384)
  })

  test('default config has conservative settings', async () => {
    const config = await getOllamaCloudConfig('default')
    expect(config.maxOutputTokens).toBe(8_192)
    expect(config.chunkSize).toBe(30_000)
    expect(config.mergeMaxOutput).toBe(16_384)
  })
})

describe('shouldUseParallelCompact', () => {
  test('returns false for non-cloud models', async () => {
    const result = await shouldUseParallelCompact('llama3.2:3b', 100_000)
    expect(result).toBe(false)
  })

  test('returns false when token count is below threshold', async () => {
    // glm-5:cloud has chunkSize of 40k, so needs 80k+ tokens
    const result = await shouldUseParallelCompact('glm-5:cloud', 50_000)
    expect(result).toBe(false)
  })

  test('returns true when token count exceeds threshold', async () => {
    // glm-5:cloud has chunkSize of 40k, so needs 80k+ tokens
    const result = await shouldUseParallelCompact('glm-5:cloud', 100_000)
    expect(result).toBe(true)
  })

  test('returns true for large conversations with qwen model', async () => {
    const result = await shouldUseParallelCompact('qwen3.5:397b-cloud', 150_000)
    expect(result).toBe(true)
  })

  test('returns false for small conversations with qwen model', async () => {
    const result = await shouldUseParallelCompact('qwen3.5:397b-cloud', 30_000)
    expect(result).toBe(false)
  })

  test('uses correct threshold for unknown models (default chunkSize)', async () => {
    // Unknown models fall back to default with chunkSize=30k
    // So needs 60k+ tokens for parallel compaction
    const result1 = await shouldUseParallelCompact('unknown:cloud', 50_000)
    expect(result1).toBe(false)

    const result2 = await shouldUseParallelCompact('unknown:cloud', 70_000)
    expect(result2).toBe(true)
  })
})
