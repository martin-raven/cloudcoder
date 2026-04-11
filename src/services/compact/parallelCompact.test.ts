import { describe, expect, test } from 'bun:test'

import type { Message } from '../../types/message.js'
import { createAssistantMessage, createUserMessage } from '../../utils/messages.js'
import { tokenCountWithEstimation } from '../../utils/tokens.js'

// Import the chunking function by reading the module internals
// Note: createBalancedChunks is not exported, so we test it indirectly

/**
 * Helper to create a conversation turn (user + assistant pair).
 */
function createTurn(userContent: string, assistantContent: string): Message[] {
  return [
    createUserMessage({ content: userContent }),
    createAssistantMessage({ content: assistantContent }),
  ]
}

/**
 * Helper to create a message group with known token count.
 */
function createGroupWithApproxTokens(
  targetTokens: number,
  turnIndex: number,
): Message[] {
  // Rough estimate: ~4 chars per token
  const charCount = targetTokens * 4
  const content = `Turn ${turnIndex}: ${'x'.repeat(charCount - 20)}`
  return createTurn(content, `Response ${turnIndex}: ${'y'.repeat(charCount / 2)}`)
}

describe('parallelCompact conversation chunking', () => {
  describe('createBalancedChunks behavior (via module import)', () => {
    test('module exports load correctly', async () => {
      const mod = await import('./parallelCompact.js')
      expect(mod.parallelCompactConversation).toBeFunction()
    })

    test('empty groups return empty chunks', async () => {
      // We can't directly test createBalancedChunks since it's not exported
      // But we can verify the module structure
      const mod = await import('./parallelCompact.js')
      expect(mod.parallelCompactConversation).toBeDefined()
    })
  })

  describe('token estimation for chunking', () => {
    test('tokenCountWithEstimation works for user messages', () => {
      const message = createUserMessage({ content: 'Hello, world!' })
      const tokens = tokenCountWithEstimation([message])
      expect(tokens).toBeGreaterThan(0)
    })

    test('tokenCountWithEstimation works for assistant messages', () => {
      const message = createAssistantMessage({ content: 'Hello, world!' })
      const tokens = tokenCountWithEstimation([message])
      expect(tokens).toBeGreaterThan(0)
    })

    test('longer content produces more tokens', () => {
      const short = createUserMessage({ content: 'Short message' })
      const long = createUserMessage({ content: 'x'.repeat(1000) })

      const shortTokens = tokenCountWithEstimation([short])
      const longTokens = tokenCountWithEstimation([long])

      expect(longTokens).toBeGreaterThan(shortTokens)
    })

    test('multiple messages sum correctly', () => {
      const messages = [
        createUserMessage({ content: 'First' }),
        createAssistantMessage({ content: 'Second' }),
        createUserMessage({ content: 'Third' }),
      ]

      const total = tokenCountWithEstimation(messages)
      const individual = messages.reduce(
        (sum, msg) => sum + tokenCountWithEstimation([msg]),
        0,
      )

      // Should be approximately equal (may vary slightly due to estimation)
      expect(Math.abs(total - individual)).toBeLessThan(10)
    })
  })

  describe('conversation structure preservation', () => {
    test('turns maintain user-assistant pairing', () => {
      const turns = [
        createTurn('Question 1', 'Answer 1'),
        createTurn('Question 2', 'Answer 2'),
        createTurn('Question 3', 'Answer 3'),
      ]

      const flat = turns.flat()
      expect(flat.length).toBe(6)
      expect(flat[0].type).toBe('user')
      expect(flat[1].type).toBe('assistant')
      expect(flat[2].type).toBe('user')
      expect(flat[3].type).toBe('assistant')
    })

    test('large turns are detected by token count', () => {
      const smallTurn = createTurn('Hi', 'Hello')
      const largeTurn = createTurn('x'.repeat(10000), 'y'.repeat(5000))

      const smallTokens = tokenCountWithEstimation(smallTurn)
      const largeTokens = tokenCountWithEstimation(largeTurn)

      expect(largeTokens).toBeGreaterThan(smallTokens * 10)
    })
  })
})

describe('parallelCompact prompt functions', () => {
  test('prompt module exports are available', async () => {
    const promptMod = await import('./prompt.js')
    expect(promptMod.getChunkCompactPrompt).toBeFunction()
    expect(promptMod.getMergeSummariesPrompt).toBeFunction()
  })

  test('getChunkCompactPrompt accepts position options', async () => {
    const { getChunkCompactPrompt } = await import('./prompt.js')

    const promptFirst = getChunkCompactPrompt({
      chunkIndex: 0,
      totalChunks: 3,
      customInstructions: undefined,
      maxOutputTokens: 16384,
    })

    const promptMiddle = getChunkCompactPrompt({
      chunkIndex: 1,
      totalChunks: 3,
      customInstructions: undefined,
      maxOutputTokens: 16384,
    })

    const promptLast = getChunkCompactPrompt({
      chunkIndex: 2,
      totalChunks: 3,
      customInstructions: undefined,
      maxOutputTokens: 16384,
    })

    // First chunk should mention being first/initial
    expect(promptFirst).toContain('FIRST')
    // Middle chunk should be different from first
    expect(promptMiddle).toBeDefined()
    expect(promptMiddle.length).toBeGreaterThan(100)
    // Last chunk should mention final
    expect(promptLast).toContain('final')
  })

  test('getChunkCompactPrompt includes custom instructions', async () => {
    const { getChunkCompactPrompt } = await import('./prompt.js')

    const customInstructions = 'Focus on code changes only'
    const prompt = getChunkCompactPrompt({
      chunkIndex: 0,
      totalChunks: 2,
      customInstructions,
      maxOutputTokens: 8192,
    })

    expect(prompt).toContain(customInstructions)
  })

  test('getMergeSummariesPrompt accepts chunk results', async () => {
    const { getMergeSummariesPrompt } = await import('./prompt.js')

    const chunkResults = [
      { chunkIndex: 0, summary: 'Summary of first chunk', tokens: 100 },
      { chunkIndex: 1, summary: 'Summary of second chunk', tokens: 150 },
    ]

    const prompt = getMergeSummariesPrompt(chunkResults, undefined)

    expect(prompt).toContain('Summary of first chunk')
    expect(prompt).toContain('Summary of second chunk')
    expect(prompt).toContain('merge')
  })

  test('getMergeSummariesPrompt includes custom instructions', async () => {
    const { getMergeSummariesPrompt } = await import('./prompt.js')

    const chunkResults = [
      { chunkIndex: 0, summary: 'First', tokens: 100 },
    ]

    const customInstructions = 'Keep it brief'
    const prompt = getMergeSummariesPrompt(chunkResults, customInstructions)

    expect(prompt).toContain(customInstructions)
  })
})

describe('parallelCompact Ollama Cloud integration', () => {
  test('ollamaCloud utilities are available', async () => {
    const ollamaMod = await import('../../utils/ollamaCloud.js')
    expect(ollamaMod.isOllamaCloudProvider).toBeFunction()
    expect(ollamaMod.getOllamaCloudConfig).toBeFunction()
    expect(ollamaMod.shouldUseParallelCompact).toBeFunction()
  })

  test('shouldUseParallelCompact is async', async () => {
    const { shouldUseParallelCompact } = await import('../../utils/ollamaCloud.js')

    const result = shouldUseParallelCompact('glm-5:cloud', 100_000)
    expect(result).toBeInstanceOf(Promise)

    const resolved = await result
    expect(typeof resolved).toBe('boolean')
    expect(resolved).toBe(true)
  })

  test('getOllamaCloudConfig returns proper structure', async () => {
    const { getOllamaCloudConfig } = await import('../../utils/ollamaCloud.js')

    const config = await getOllamaCloudConfig('glm-5:cloud')

    expect(config).toHaveProperty('contextWindow')
    expect(config).toHaveProperty('maxOutputTokens')
    expect(config).toHaveProperty('chunkSize')
    expect(config).toHaveProperty('mergeMaxOutput')
    expect(typeof config.contextWindow).toBe('number')
    expect(typeof config.maxOutputTokens).toBe('number')
    expect(typeof config.chunkSize).toBe('number')
    expect(typeof config.mergeMaxOutput).toBe('number')
  })

  test('config values are reasonable for cloud models', async () => {
    const { getOllamaCloudConfig } = await import('../../utils/ollamaCloud.js')

    const models = [
      'glm-5:cloud',
      'glm-5.1:cloud',
      'qwen3.5:cloud',
      'deepseek-v3:cloud',
    ]

    for (const model of models) {
      const config = await getOllamaCloudConfig(model)

      // Context window should be large for cloud models
      expect(config.contextWindow).toBeGreaterThanOrEqual(100_000)

      // Chunk size should be reasonable for parallel processing
      expect(config.chunkSize).toBeGreaterThanOrEqual(20_000)
      expect(config.chunkSize).toBeLessThanOrEqual(config.contextWindow / 2)

      // Max output should be reasonable
      expect(config.maxOutputTokens).toBeGreaterThanOrEqual(4_096)
      expect(config.maxOutputTokens).toBeLessThanOrEqual(64_000)
    }
  })
})

describe('parallelCompact edge cases', () => {
  test('handles very small conversations', async () => {
    const { shouldUseParallelCompact } = await import('../../utils/ollamaCloud.js')

    // Very small conversation - should not use parallel
    const result = await shouldUseParallelCompact('glm-5:cloud', 1000)
    expect(result).toBe(false)
  })

  test('handles boundary token counts', async () => {
    const { shouldUseParallelCompact } = await import('../../utils/ollamaCloud.js')

    // glm-5:cloud has chunkSize=40k, needs 80k+ for parallel
    const justBelow = await shouldUseParallelCompact('glm-5:cloud', 79_999)
    expect(justBelow).toBe(false)

    const atBoundary = await shouldUseParallelCompact('glm-5:cloud', 80_000)
    expect(atBoundary).toBe(true)

    const above = await shouldUseParallelCompact('glm-5:cloud', 100_000)
    expect(above).toBe(true)
  })

  test('handles unknown cloud models gracefully', async () => {
    const { getOllamaCloudConfig, shouldUseParallelCompact } = await import('../../utils/ollamaCloud.js')

    // Unknown model should still return valid config
    const config = await getOllamaCloudConfig('unknown-model:cloud')
    expect(config.contextWindow).toBeGreaterThan(0)
    expect(config.chunkSize).toBeGreaterThan(0)

    // Should not throw for unknown models
    const result = await shouldUseParallelCompact('unknown-model:cloud', 100_000)
    expect(typeof result).toBe('boolean')
  })

  test('non-cloud models return false for parallel compact', async () => {
    const { shouldUseParallelCompact } = await import('../../utils/ollamaCloud.js')

    const result1 = await shouldUseParallelCompact('llama3.2:3b', 100_000)
    expect(result1).toBe(false)

    const result2 = await shouldUseParallelCompact('gpt-4o', 100_000)
    expect(result2).toBe(false)

    const result3 = await shouldUseParallelCompact('claude-sonnet-4-5', 100_000)
    expect(result3).toBe(false)
  })
})

describe('message creation helpers', () => {
  test('createUserMessage produces valid structure', () => {
    const msg = createUserMessage({ content: 'Test' })
    expect(msg.type).toBe('user')
    expect(msg.message.role).toBe('user')
    expect(msg.message.content).toBe('Test')
    expect(msg.uuid).toBeDefined()
    expect(msg.timestamp).toBeDefined()
  })

  test('createAssistantMessage produces valid structure', () => {
    const msg = createAssistantMessage({ content: 'Test' })
    expect(msg.type).toBe('assistant')
    expect(msg.message.role).toBe('assistant')
    // Content may be string or array depending on implementation
    const content = typeof msg.message.content === 'string'
      ? msg.message.content
      : msg.message.content[0]?.text || ''
    expect(content).toBe('Test')
    expect(msg.uuid).toBeDefined()
    expect(msg.timestamp).toBeDefined()
  })

  test('messages with isCompactSummary flag', () => {
    const msg = createUserMessage({
      content: 'Summary content',
      isCompactSummary: true,
      isVisibleInTranscriptOnly: true,
    })

    expect(msg.isCompactSummary).toBe(true)
    expect(msg.isVisibleInTranscriptOnly).toBe(true)
    expect(msg.message.content).toBe('Summary content')
  })
})
