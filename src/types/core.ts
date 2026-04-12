/**
 * Core types shared across all modules.
 * This file has NO dependencies on other src/ modules to prevent circular imports.
 * Only imports from: node standard library, external packages (crypto, zod)
 */

import type { UUID } from 'crypto';
import type { z } from 'zod/v4';

// ============================================================================
// Tool Types
// ============================================================================

export type ToolName = string;

export type ToolPermissionBehavior = 'allow' | 'deny' | 'ask';

export interface ToolPermissionResult {
  behavior: ToolPermissionBehavior;
  updatedInput?: Record<string, unknown>;
  reason?: string;
}

export interface ToolProgressData {
  type: string;
  data: unknown;
}

// ============================================================================
// Event Types
// ============================================================================

export type EventType =
  | 'tool_call_start'
  | 'tool_call_complete'
  | 'tool_call_error'
  | 'permission_check'
  | 'api_request_start'
  | 'api_request_complete'
  | 'context_compact_start'
  | 'context_compact_complete'
  | 'session_start'
  | 'session_end'
  | 'settings_change';

export interface CloudCoderEvent<T = unknown> {
  type: EventType;
  payload: T;
  timestamp: number;
  source: string;
}

export type EventHandler<T = unknown> = (event: CloudCoderEvent<T>) => void | Promise<void>;

// ============================================================================
// Service Types
// ============================================================================

export interface HealthStatus {
  healthy: boolean;
  checks: Map<string, { ok: boolean; message?: string }>;
  lastCheck: number;
}

export interface Service {
  name: string;
  initialize(): Promise<void>;
  dispose(): Promise<void>;
  healthCheck(): Promise<HealthStatus>;
}

// ============================================================================
// Error Types
// ============================================================================

export class CloudCoderError extends Error {
  public readonly code: string;
  public readonly context?: Record<string, unknown>;

  constructor(message: string, code: string, context?: Record<string, unknown>) {
    super(message);
    this.name = 'CloudCoderError';
    this.code = code;
    this.context = context;
  }
}

export class ToolExecutionError extends CloudCoderError {
  constructor(
    message: string,
    public readonly toolName: string,
    public readonly toolInput?: unknown
  ) {
    super(message, 'TOOL_EXECUTION_ERROR', { toolName, toolInput });
    this.name = 'ToolExecutionError';
  }
}

export class PermissionDeniedError extends CloudCoderError {
  constructor(
    message: string,
    public readonly toolName: string,
    public readonly reason?: string
  ) {
    super(message, 'PERMISSION_DENIED', { toolName, reason });
    this.name = 'PermissionDeniedError';
  }
}

// ============================================================================
// Result Types
// ============================================================================

export type Result<T, E = Error> =
  | { ok: true; value: T }
  | { ok: false; error: E };

export function ok<T>(value: T): Result<T, never> {
  return { ok: true, value };
}

export function err<E>(error: E): Result<never, E> {
  return { ok: false, error };
}

// ============================================================================
// Lazy Loading Types
// ============================================================================

export interface LazyLoader<T> {
  (): Promise<T>;
}

export interface LazyRegistryStats {
  totalKeys: number;
  loadedKeys: number;
  pendingKeys: number;
  failedKeys: number;
}

// ============================================================================
// Cache Types
// ============================================================================

export interface CacheStats {
  size: number;
  hits: number;
  misses: number;
  evictions: number;
  hitRate: number;
}

export interface CacheOptions {
  maxSize: number;
  ttlMs: number;
}
