/**
 * Event bus types for cross-module communication.
 * Replaces direct imports that cause circular dependencies.
 *
 * Import only from ./core.ts to maintain isolation.
 */

import type { CloudCoderEvent, EventHandler, EventType, EventBusStats } from './core.js';

export interface EventSubscription {
  unsubscribe(): void;
}

export interface EventBusOptions {
  /** Maximum events to buffer for late subscribers */
  maxBufferedEvents?: number;
  /** Enable debug logging */
  debug?: boolean;
}

export interface IEventBus {
  /** Subscribe to events of a specific type */
  subscribe<T>(type: EventType, handler: EventHandler<T>): EventSubscription;

  /** Subscribe to all events */
  subscribeAll(handler: EventHandler): EventSubscription;

  /** Emit an event to all subscribers */
  emit<T>(event: CloudCoderEvent<T>): void;

  /** Get statistics about event emission */
  getStats(): EventBusStats;

  /** Clear all subscriptions (for testing) */
  clear(): void;
}

/**
 * Event types grouped by category for easier subscription management.
 */
export const EventCategories = {
  /** Tool lifecycle events */
  TOOL: ['tool_call_start', 'tool_call_complete', 'tool_call_error'] as EventType[],

  /** Permission-related events */
  PERMISSION: ['permission_check'] as EventType[],

  /** API request events */
  API: ['api_request_start', 'api_request_complete'] as EventType[],

  /** Context management events */
  CONTEXT: ['context_compact_start', 'context_compact_complete'] as EventType[],

  /** Session lifecycle events */
  SESSION: ['session_start', 'session_end'] as EventType[],

  /** Settings events */
  SETTINGS: ['settings_change'] as EventType[],
} as const;

/**
 * Helper type to extract payload type for a given event type.
 */
export type EventPayload<T extends EventType> = CloudCoderEvent<infer P> extends { type: T } ? P : never;

/**
 * Typed event handler that infers payload type from event type.
 */
export type TypedEventHandler<T extends EventType> = (event: CloudCoderEvent<EventPayload<T>>) => void | Promise<void>;
