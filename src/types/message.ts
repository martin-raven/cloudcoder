/**
 * Message types for Cloud Coder.
 *
 * This file is a stub for the open-source version. In the full Anthropic
 * codebase, these types are generated from internal schemas.
 */

import type { UUID } from 'crypto'
import type { ToolUseBlockParam, ToolResultBlockParam } from '@anthropic-ai/sdk/resources/index.mjs'

export type MessageBase = {
  uuid: UUID
  timestamp: number
  isMeta?: boolean
}

export type UserMessage = MessageBase & {
  type: 'user'
  message: {
    role: 'user'
    content: string | Array<ContentBlockParam>
    id?: string
  }
  isCompactSummary?: boolean
  isVisibleInTranscriptOnly?: boolean
  summarizeMetadata?: {
    messagesSummarized: number
    userContext?: string
    direction: PartialCompactDirection
  }
}

export type AssistantMessage = MessageBase & {
  type: 'assistant'
  message: {
    id: string
    role: 'assistant'
    content: string | Array<ContentBlockParam>
    usage?: {
      input_tokens: number
      output_tokens: number
      cache_creation_input_tokens?: number
      cache_read_input_tokens?: number
    }
  }
  isApiErrorMessage?: boolean
}

export type SystemMessage = MessageBase & {
  type: 'system'
  message?: {
    role: 'system'
    content?: string
  }
  subtype?: string
  attachment?: Attachment
  compactMetadata?: {
    trigger: 'auto' | 'manual'
    preCompactTokenCount: number
    preservedSegment?: {
      headUuid: UUID
      anchorUuid: UUID
      tailUuid: UUID
    }
    preCompactDiscoveredTools?: string[]
  }
}

export type SystemCompactBoundaryMessage = SystemMessage & {
  subtype: 'compact_boundary'
  compactMetadata: {
    trigger: 'auto' | 'manual'
    preCompactTokenCount: number
    preservedSegment?: {
      headUuid: UUID
      anchorUuid: UUID
      tailUuid: UUID
    }
    preCompactDiscoveredTools?: string[]
  }
}

export type AttachmentMessage = MessageBase & {
  type: 'attachment'
  attachment: Attachment
}

export type Attachment =
  | {
      type: 'file_reference'
      filePath: string
      content?: string
    }
  | {
      type: 'plan_file_reference'
      planFilePath: string
      planContent: string
    }
  | {
      type: 'invoked_skills'
      skills: Array<{ name: string; path: string; content: string }>
    }
  | {
      type: 'plan_mode'
      reminderType: 'full' | 'brief'
      isSubAgent: boolean
      planFilePath: string
      planExists: boolean
    }
  | {
      type: 'task_status'
      taskId: string
      taskType: string
      description: string
      status: string
      deltaSummary?: string | null
      outputFilePath?: string
    }
  | {
      type: 'skill_discovery'
      skills: Array<{ name: string; description: string }>
    }
  | {
      type: 'skill_listing'
      skills: Array<{ name: string; description: string }>
    }

export type HookResultMessage = MessageBase & {
  type: 'hook_result'
  hookType: 'pre_compact' | 'post_compact' | 'session_start'
  content: string
}

export type ProgressMessage = MessageBase & {
  type: 'progress'
  progress: {
    type: string
    data: unknown
  }
}

export type SystemLocalCommandMessage = MessageBase & {
  type: 'system'
  subtype: 'local_command'
  command: string
}

export type Message =
  | UserMessage
  | AssistantMessage
  | SystemMessage
  | SystemCompactBoundaryMessage
  | AttachmentMessage
  | HookResultMessage
  | ProgressMessage
  | SystemLocalCommandMessage

export type ContentBlockParam =
  | { type: 'text'; text: string }
  | { type: 'image'; source: { type: 'base64' | 'url'; data?: string; media_type?: string; url?: string } }
  | { type: 'document'; source: { type: 'base64' | 'url'; data?: string; media_type?: string; url?: string } }
  | ToolUseBlockParam
  | ToolResultBlockParam
  | { type: 'thinking'; thinking: string; signature?: string }
  | { type: 'redacted_thinking'; data: string }

export type PartialCompactDirection = 'from' | 'up_to'

export function createCompactBoundaryMessage(
  trigger: 'auto' | 'manual',
  preCompactTokenCount: number,
  lastMessageUuid?: UUID,
): SystemCompactBoundaryMessage {
  return {
    type: 'system',
    subtype: 'compact_boundary',
    uuid: crypto.randomUUID() as UUID,
    timestamp: Date.now(),
    compactMetadata: {
      trigger,
      preCompactTokenCount,
    },
  }
}

export function createUserMessage(options: {
  content: string | Array<ContentBlockParam>
  isCompactSummary?: boolean
  isVisibleInTranscriptOnly?: boolean
  summarizeMetadata?: UserMessage['summarizeMetadata']
}): UserMessage {
  return {
    type: 'user',
    uuid: crypto.randomUUID() as UUID,
    timestamp: Date.now(),
    message: {
      role: 'user',
      content: options.content,
    },
    isCompactSummary: options.isCompactSummary,
    isVisibleInTranscriptOnly: options.isVisibleInTranscriptOnly,
    summarizeMetadata: options.summarizeMetadata,
  }
}

export function createAttachmentMessage(attachment: Attachment): AttachmentMessage {
  return {
    type: 'attachment',
    uuid: crypto.randomUUID() as UUID,
    timestamp: Date.now(),
    attachment,
  }
}
