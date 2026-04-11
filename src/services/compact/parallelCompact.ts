/**
 * Parallel chunked compaction for Ollama Cloud models.
 *
 * Since Ollama Cloud doesn't support prompt caching, we optimize by:
 * 1. Splitting large conversations into chunks
 * 2. Compacting each chunk in parallel
 * 3. Merging the partial summaries into a final summary
 *
 * This approach is faster than sequential compaction for large conversations
 * because multiple API calls run concurrently.
 */

import type { ToolUseContext } from '../../Tool.js'
import type {
  AssistantMessage,
  Message,
  UserMessage,
  SystemCompactBoundaryMessage,
} from '../../types/message.js'
import {
  createUserMessage,
  getAssistantMessageText,
  createCompactBoundaryMessage,
} from '../../utils/messages.js'
import { roughTokenCountEstimation } from '../../services/tokenEstimation.js'
import {
  type CompactionResult,
  ERROR_MESSAGE_INCOMPLETE_RESPONSE,
  ERROR_MESSAGE_NOT_ENOUGH_MESSAGES,
  mergeHookInstructions,
} from './compact.js'
import {
  getOllamaCloudConfig,
  isOllamaCloudProvider,
} from '../../utils/ollamaCloud.js'
import { groupMessagesByApiRound } from './grouping.js'
import { getChunkCompactPrompt, getMergeSummariesPrompt } from './prompt.js'
import { logEvent } from '../analytics/index.js'
import { getTokenUsage, tokenCountWithEstimation } from '../../utils/tokens.js'
import { logError } from '../../utils/log.js'
import { getMaxOutputTokensForModel } from '../api/claude.js'
import { normalizeMessagesForAPI } from '../../utils/messages.js'
import { asSystemPrompt } from '../../utils/systemPromptType.js'
import { queryModelWithStreaming } from '../api/claude.js'
import { FileReadTool } from '../../tools/FileReadTool/FileReadTool.js'
import { stripImagesFromMessages, stripReinjectedAttachments } from './compact.js'
import type { CacheSafeParams } from '../../utils/forkedAgent.js'
import { executePreCompactHooks, executePostCompactHooks } from '../../utils/hooks.js'
import { processSessionStartHooks } from '../../utils/sessionStart.js'
import { runPostCompactCleanup } from './postCompactCleanup.js'
import { getUserContext } from '../../context.js'
import { markPostCompaction } from '../../bootstrap/state.js'
import { notifyCompaction } from '../api/promptCacheBreakDetection.js'
import { feature } from 'bun:bundle'

/**
 * Result from compacting a single chunk.
 */
interface ChunkResult {
  chunkIndex: number
  summary: string
  tokens: number
  usage?: ReturnType<typeof getTokenUsage>
}

/**
 * Parallel compaction for Ollama Cloud models.
 *
 * Splits messages into chunks, compacts each in parallel, then merges summaries.
 * Falls back to standard compaction if:
 * - Not an Ollama Cloud model
 * - Conversation too small to benefit from parallelization
 * - Only 1 chunk would result
 */
export async function parallelCompactConversation(
  messages: Message[],
  context: ToolUseContext,
  cacheSafeParams: CacheSafeParams,
  isAutoCompact: boolean,
  customInstructions?: string,
): Promise<CompactionResult> {
  const model = context.options.mainLoopModel

  // Guard: only for Ollama Cloud
  if (!isOllamaCloudProvider(model)) {
    throw new Error('parallelCompactConversation requires an Ollama Cloud model')
  }

  const config = await getOllamaCloudConfig(model)
  const preCompactTokenCount = tokenCountWithEstimation(messages)

  context.onCompactProgress?.({
    type: 'hooks_start',
    hookType: 'pre_compact',
  })

  context.setSDKStatus?.('compacting')

  // Execute PreCompact hooks
  const hookResult = await executePreCompactHooks(
    {
      trigger: isAutoCompact ? 'auto' : 'manual',
      customInstructions: customInstructions ?? null,
    },
    context.abortController.signal,
  )
  customInstructions = mergeHookInstructions(
    customInstructions,
    hookResult.newCustomInstructions,
  )
  const userDisplayMessage = hookResult.userDisplayMessage

  // Show requesting mode
  context.setStreamMode?.('requesting')
  context.setResponseLength?.(() => 0)
  context.onCompactProgress?.({ type: 'compact_start' })

  try {
    // Step 1: Group messages by API round (preserves conversation structure)
    const groups = groupMessagesByApiRound(messages)

    if (groups.length < 2) {
      throw new Error(ERROR_MESSAGE_NOT_ENOUGH_MESSAGES)
    }

    // Step 2: Create balanced chunks
    const chunks = createBalancedChunks(groups, config.chunkSize)

    // If only 1 chunk, fall back to standard compaction
    if (chunks.length <= 1) {
      const { compactConversation } = await import('./compact.js')
      return compactConversation(
        messages,
        context,
        cacheSafeParams,
        true, // suppressFollowUpQuestions
        customInstructions,
        isAutoCompact,
      )
    }

    // Step 3: Compact each chunk in parallel
    const chunkResults = await compactChunksInParallel(
      chunks,
      context,
      cacheSafeParams,
      customInstructions,
      config.maxOutputTokens,
    )

    // Step 4: Merge partial summaries
    const mergedSummary = await mergeChunkSummaries(
      chunkResults,
      context,
      cacheSafeParams,
      config.mergeMaxOutput,
      customInstructions,
    )

    // Step 5: Build final result
    const boundaryMarker = createCompactBoundaryMessage(
      isAutoCompact ? 'auto' : 'manual',
      preCompactTokenCount,
      messages.at(-1)?.uuid,
    )

    const summaryMessages: UserMessage[] = [
      createUserMessage({
        content: mergedSummary,
        isCompactSummary: true,
        isVisibleInTranscriptOnly: true,
      }),
    ]

    // Execute PostCompact hooks
    context.onCompactProgress?.({
      type: 'hooks_start',
      hookType: 'post_compact',
    })
    const postCompactHookResult = await executePostCompactHooks(
      {
        trigger: isAutoCompact ? 'auto' : 'manual',
        compactSummary: mergedSummary,
      },
      context.abortController.signal,
    )

    // Execute SessionStart hooks
    context.onCompactProgress?.({
      type: 'hooks_start',
      hookType: 'session_start',
    })
    const hookMessages = await processSessionStartHooks('compact', {
      model: context.options.mainLoopModel,
    })

    // Calculate post-compact token estimates
    const truePostCompactTokenCount = tokenCountWithEstimation([
      boundaryMarker,
      ...summaryMessages,
      ...hookMessages,
    ])

    // Log analytics
    logEvent('tengu_parallel_compact', {
      preCompactTokenCount,
      truePostCompactTokenCount,
      isAutoCompact,
      chunkCount: chunks.length,
    })

    // Mark post-compaction for cache break detection
    if (feature('PROMPT_CACHE_BREAK_DETECTION')) {
      notifyCompaction(context.options.querySource ?? 'compact', context.agentId)
    }
    markPostCompaction()

    // Clear caches
    getUserContext.cache.clear?.()
    runPostCompactCleanup(context.options.querySource)

    const combinedUserDisplayMessage = [
      userDisplayMessage,
      postCompactHookResult.userDisplayMessage,
    ]
      .filter(Boolean)
      .join('\n')

    return {
      boundaryMarker,
      summaryMessages,
      attachments: [],
      hookResults: hookMessages,
      preCompactTokenCount,
      postCompactTokenCount: preCompactTokenCount,
      truePostCompactTokenCount,
      userDisplayMessage: combinedUserDisplayMessage || undefined,
    }
  } catch (error) {
    logError(error as Error)
    throw error
  } finally {
    context.setStreamMode?.('requesting')
    context.setResponseLength?.(() => 0)
    context.onCompactProgress?.({ type: 'compact_end' })
    context.setSDKStatus?.(null)
  }
}

/**
 * Split message groups into balanced chunks.
 * Each chunk targets ~chunkSize tokens but respects API-round boundaries.
 */
function createBalancedChunks(
  groups: Message[][],
  targetChunkSize: number,
): Message[][] {
  const chunks: Message[][] = []
  let currentChunk: Message[] = []
  let currentTokens = 0

  for (const group of groups) {
    const groupTokens = tokenCountWithEstimation(group)

    // If adding this group exceeds target and we have content, start new chunk
    if (currentTokens + groupTokens > targetChunkSize && currentChunk.length > 0) {
      chunks.push(currentChunk)
      currentChunk = []
      currentTokens = 0
    }

    currentChunk.push(...group)
    currentTokens += groupTokens
  }

  // Don't forget the last chunk
  if (currentChunk.length > 0) {
    chunks.push(currentChunk)
  }

  return chunks
}

/**
 * Compact all chunks in parallel.
 */
async function compactChunksInParallel(
  chunks: Message[][],
  context: ToolUseContext,
  cacheSafeParams: CacheSafeParams,
  customInstructions: string | undefined,
  maxOutputTokens: number,
): Promise<ChunkResult[]> {
  const results = await Promise.all(
    chunks.map((chunk, index) =>
      compactChunk(chunk, context, cacheSafeParams, {
        chunkIndex: index,
        totalChunks: chunks.length,
        customInstructions,
        maxOutputTokens,
      }).catch(error => {
        logError(error as Error)
        throw error
      }),
    ),
  )

  return results
}

/**
 * Compact a single chunk with chunk-aware prompt.
 */
async function compactChunk(
  messages: Message[],
  context: ToolUseContext,
  cacheSafeParams: CacheSafeParams,
  options: {
    chunkIndex: number
    totalChunks: number
    customInstructions?: string
    maxOutputTokens: number
  },
): Promise<ChunkResult> {
  const prompt = getChunkCompactPrompt(options)
  const summaryRequest = createUserMessage({ content: prompt })

  const response = await streamCompactSummaryDirect(
    messages,
    summaryRequest,
    context,
    cacheSafeParams,
    options.maxOutputTokens,
  )

  const summary = getAssistantMessageText(response)
  const usage = getTokenUsage(response)

  if (!summary) {
    throw new Error(ERROR_MESSAGE_INCOMPLETE_RESPONSE)
  }

  return {
    chunkIndex: options.chunkIndex,
    summary,
    tokens: tokenCountWithEstimation([response]),
    usage,
  }
}

/**
 * Merge summaries from all chunks into a coherent final summary.
 */
async function mergeChunkSummaries(
  chunkResults: ChunkResult[],
  context: ToolUseContext,
  cacheSafeParams: CacheSafeParams,
  maxOutputTokens: number,
  customInstructions?: string,
): Promise<string> {
  // Sort by chunk index to preserve chronological order
  const sorted = chunkResults.sort((a, b) => a.chunkIndex - b.chunkIndex)

  // Build the merge prompt
  const prompt = getMergeSummariesPrompt(sorted, customInstructions)
  const summaryRequest = createUserMessage({ content: prompt })

  const response = await streamCompactSummaryDirect(
    [summaryRequest],
    summaryRequest,
    context,
    cacheSafeParams,
    maxOutputTokens,
  )

  const summary = getAssistantMessageText(response)

  if (!summary) {
    throw new Error(ERROR_MESSAGE_INCOMPLETE_RESPONSE)
  }

  return summary
}

/**
 * Direct streaming compaction without forked agent.
 * Used for Ollama Cloud where prompt cache is not available.
 */
async function streamCompactSummaryDirect(
  messages: Message[],
  summaryRequest: UserMessage,
  context: ToolUseContext,
  cacheSafeParams: CacheSafeParams,
  maxOutputTokens: number,
): Promise<AssistantMessage> {
  const appState = context.getAppState()

  // For Ollama Cloud, skip the forked agent path (no cache benefit)
  // Go straight to streaming

  const streamingGen = queryModelWithStreaming({
    messages: normalizeMessagesForAPI(
      stripImagesFromMessages(
        stripReinjectedAttachments([
          ...messages,
          summaryRequest,
        ]),
      ),
      context.options.tools,
    ),
    systemPrompt: asSystemPrompt([
      'You are a helpful AI assistant tasked with summarizing conversations.',
    ]),
    thinkingConfig: { type: 'disabled' as const },
    tools: [FileReadTool],
    signal: context.abortController.signal,
    options: {
      async getToolPermissionContext() {
        const appState = context.getAppState()
        return appState.toolPermissionContext
      },
      model: context.options.mainLoopModel,
      toolChoice: undefined,
      isNonInteractiveSession: context.options.isNonInteractiveSession,
      hasAppendSystemPrompt: !!context.options.appendSystemPrompt,
      maxOutputTokensOverride: Math.min(
        maxOutputTokens,
        getMaxOutputTokensForModel(context.options.mainLoopModel),
      ),
      querySource: 'compact',
      agents: context.options.agentDefinitions.activeAgents,
      mcpTools: [],
      effortValue: appState.effortValue,
    },
  })

  const streamIter = streamingGen[Symbol.asyncIterator]()
  let next = await streamIter.next()
  let response: AssistantMessage | undefined

  while (!next.done) {
    const event = next.value

    if (event.type === 'assistant') {
      response = event
    }

    next = await streamIter.next()
  }

  if (!response) {
    throw new Error(ERROR_MESSAGE_INCOMPLETE_RESPONSE)
  }

  return response
}

