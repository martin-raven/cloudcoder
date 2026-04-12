# Cloud Coder Architecture Improvements Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement all architecture improvements, new services, new tools, performance optimizations, and security enhancements recommended in improvements.html

**Architecture:** Modular, service-oriented architecture with lazy loading, event-driven communication, and comprehensive caching. Each service and tool follows a consistent interface pattern with proper isolation and health monitoring.

**Tech Stack:** TypeScript (current), with Rust FFI bindings for performance-critical paths (Phase 2), SQLite for caching, Ink/React for UI

**Total Estimated Effort:** 12 months across 4 phases

**Phase Breakdown:**
- Phase 1 (Months 1-3): Foundation - 45 tasks
- Phase 2 (Months 4-6): Performance - 38 tasks
- Phase 3 (Months 7-9): Features - 52 tasks
- Phase 4 (Months 10-12): Polish - 25 tasks

---

## File Structure Overview

### New Directories to Create
```
src/
├── types/
│   └── core.ts              # Shared types to eliminate circular deps
├── services/
│   ├── cache/               # Caching service
│   ├── telemetry/           # Telemetry service
│   ├── rate-limiter/        # Rate limiting service
│   ├── secrets/             # Secret management
│   ├── health/              # Health monitoring
│   ├── updater/             # Update service
│   ├── extension-host/      # Extension hosting
│   └── vector-store/        # Vector embeddings
├── tools/
│   ├── DatabaseTool/
│   ├── DockerTool/
│   ├── HttpTool/
│   ├── DiffTool/
│   ├── ProcessTool/
│   ├── NetworkTool/
│   ├── CryptoTool/
│   ├── ArchiveTool/
│   └── LogTool/
├── utils/
│   ├── events/              # Event bus implementation
│   └── lazy/                # Lazy loading utilities
└── rust/                    # Rust FFI bindings (Phase 2)
    ├── src/
    ├── Cargo.toml
    └── build.rs
```

### Core Files to Modify
```
src/Tool.ts                  # Add lazy loading interface
src/tools.ts                 # Implement lazy tool registry
src/main.tsx                 # Add service initialization
src/entrypoints/cli.tsx      # Add lazy loading bootstrap
src/utils/permissions/permissions.ts  # Fix circular imports
src/state/AppState.tsx       # Event bus integration
scripts/build.ts             # Modular build configuration
```

---

## Phase 1: Foundation (Months 1-3)

### Task 1: Create Shared Core Types Module

**Goal:** Eliminate circular dependencies by creating a shared types package

**Files:**
- Create: `src/types/core.ts`
- Create: `src/types/events.ts`
- Modify: `src/Tool.ts:1-50` (import from core)

- [ ] **Step 1: Create src/types/core.ts with base types**

```typescript
// src/types/core.ts
/**
 * Core types shared across all modules.
 * This file has NO dependencies on other src/ modules to prevent circular imports.
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

export interface Result<T, E = Error> {
  ok: true;
  value: T;
} | {
  ok: false;
  error: E;
}

export function ok<T>(value: T): Result<T, never> {
  return { ok: true, value };
}

export function err<E>(error: E): Result<never, E> {
  return { ok: false, error };
}
```

- [ ] **Step 2: Create src/types/events.ts with event bus types**

```typescript
// src/types/events.ts
/**
 * Event bus types for cross-module communication.
 * Replaces direct imports that cause circular dependencies.
 */

import type { CloudCoderEvent, EventHandler, EventType } from './core.js';

export interface EventSubscription {
  unsubscribe(): void;
}

export interface EventBusOptions {
  /** Maximum events to buffer for late subscribers */
  maxBufferedEvents?: number;
  /** Enable debug logging */
  debug?: boolean;
}

export interface EventBusStats {
  totalEventsEmitted: number;
  totalListeners: number;
  eventsByType: Map<EventType, number>;
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
```

- [ ] **Step 3: Run TypeScript to verify types compile**

```bash
bun run typecheck
```
Expected: PASS with no errors in new type files

- [ ] **Step 4: Commit**

```bash
git add src/types/core.ts src/types/events.ts
git commit -m "feat: add shared core types to eliminate circular dependencies"
```

---

### Task 2: Implement Event Bus

**Goal:** Create event bus for decoupled cross-module communication

**Files:**
- Create: `src/utils/events/EventBus.ts`
- Create: `src/utils/events/index.ts`
- Test: `src/utils/events/EventBus.test.ts`

- [ ] **Step 1: Write failing tests**

```typescript
// src/utils/events/EventBus.test.ts
import { describe, test, expect } from 'bun:test';
import { EventBus } from './EventBus.js';

describe('EventBus', () => {
  test('should subscribe and receive events', () => {
    const bus = new EventBus();
    const received: string[] = [];

    bus.subscribe('tool_call_start', (event) => {
      received.push(event.type);
    });

    bus.emit({
      type: 'tool_call_start',
      payload: { toolName: 'BashTool' },
      timestamp: Date.now(),
      source: 'test'
    });

    expect(received).toEqual(['tool_call_start']);
  });

  test('should unsubscribe correctly', () => {
    const bus = new EventBus();
    let callCount = 0;

    const subscription = bus.subscribe('tool_call_complete', () => {
      callCount++;
    });

    bus.emit({
      type: 'tool_call_complete',
      payload: {},
      timestamp: Date.now(),
      source: 'test'
    });

    subscription.unsubscribe();

    bus.emit({
      type: 'tool_call_complete',
      payload: {},
      timestamp: Date.now(),
      source: 'test'
    });

    expect(callCount).toBe(1);
  });

  test('should buffer events for late subscribers', () => {
    const bus = new EventBus({ maxBufferedEvents: 10 });

    bus.emit({
      type: 'session_start',
      payload: { sessionId: 'test-123' },
      timestamp: Date.now(),
      source: 'test'
    });

    const received: unknown[] = [];
    bus.subscribe('session_start', (event) => {
      received.push(event.payload);
    });

    expect(received).toHaveLength(1);
    expect(received[0]).toEqual({ sessionId: 'test-123' });
  });

  test('should track statistics', () => {
    const bus = new EventBus();

    bus.emit({
      type: 'tool_call_start',
      payload: {},
      timestamp: Date.now(),
      source: 'test'
    });

    bus.emit({
      type: 'tool_call_start',
      payload: {},
      timestamp: Date.now(),
      source: 'test'
    });

    bus.emit({
      type: 'api_request_complete',
      payload: {},
      timestamp: Date.now(),
      source: 'test'
    });

    const stats = bus.getStats();
    expect(stats.totalEventsEmitted).toBe(3);
    expect(stats.eventsByType.get('tool_call_start')).toBe(2);
    expect(stats.eventsByType.get('api_request_complete')).toBe(1);
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

```bash
bun test src/utils/events/EventBus.test.ts
```
Expected: FAIL with "Cannot find module './EventBus.js'"

- [ ] **Step 3: Implement EventBus**

```typescript
// src/utils/events/EventBus.ts
import type {
  CloudCoderEvent,
  EventHandler,
  EventType,
  EventBusStats,
  IEventBus,
  EventSubscription,
} from '../../types/core.js';
import type { EventBusOptions } from '../../types/events.js';

export class EventBus implements IEventBus {
  private subscribers: Map<EventType, Set<EventHandler>> = new Map();
  private allSubscribers: Set<EventHandler> = new Set();
  private eventBuffer: CloudCoderEvent[] = [];
  private stats: {
    totalEventsEmitted: number;
    eventsByType: Map<EventType, number>;
  } = {
    totalEventsEmitted: 0,
    eventsByType: new Map(),
  };
  private readonly maxBufferedEvents: number;
  private readonly debug: boolean;

  constructor(options: EventBusOptions = {}) {
    this.maxBufferedEvents = options.maxBufferedEvents ?? 100;
    this.debug = options.debug ?? false;
  }

  subscribe<T>(type: EventType, handler: EventHandler<T>): EventSubscription {
    if (!this.subscribers.has(type)) {
      this.subscribers.set(type, new Set());
    }
    this.subscribers.get(type)!.add(handler as EventHandler);

    // Deliver buffered events to new subscriber
    for (const event of this.eventBuffer) {
      if (event.type === type) {
        this.deliverEvent(handler as EventHandler, event);
      }
    }

    return {
      unsubscribe: () => {
        this.subscribers.get(type)?.delete(handler as EventHandler);
      },
    };
  }

  subscribeAll(handler: EventHandler): EventSubscription {
    this.allSubscribers.add(handler);

    // Deliver all buffered events
    for (const event of this.eventBuffer) {
      this.deliverEvent(handler, event);
    }

    return {
      unsubscribe: () => {
        this.allSubscribers.delete(handler);
      },
    };
  }

  emit<T>(event: CloudCoderEvent<T>): void {
    // Update stats
    this.stats.totalEventsEmitted++;
    const currentCount = this.stats.eventsByType.get(event.type) ?? 0;
    this.stats.eventsByType.set(event.type, currentCount + 1);

    // Buffer event
    this.eventBuffer.push(event as CloudCoderEvent);
    if (this.eventBuffer.length > this.maxBufferedEvents) {
      this.eventBuffer.shift();
    }

    // Deliver to type-specific subscribers
    const typeSubscribers = this.subscribers.get(event.type);
    if (typeSubscribers) {
      for (const handler of typeSubscribers) {
        this.deliverEvent(handler, event as CloudCoderEvent);
      }
    }

    // Deliver to catch-all subscribers
    for (const handler of this.allSubscribers) {
      this.deliverEvent(handler, event as CloudCoderEvent);
    }

    if (this.debug) {
      console.log(`[EventBus] Emitted: ${event.type}`);
    }
  }

  private async deliverEvent(handler: EventHandler, event: CloudCoderEvent): Promise<void> {
    try {
      const result = handler(event);
      if (result instanceof Promise) {
        await result.catch((err) => {
          console.error(`[EventBus] Error in event handler for ${event.type}:`, err);
        });
      }
    } catch (err) {
      console.error(`[EventBus] Error in event handler for ${event.type}:`, err);
    }
  }

  getStats(): EventBusStats {
    return {
      totalListeners: this.allSubscribers.size +
        Array.from(this.subscribers.values()).reduce((sum, set) => sum + set.size, 0),
      ...this.stats,
    };
  }

  clear(): void {
    this.subscribers.clear();
    this.allSubscribers.clear();
    this.eventBuffer = [];
    this.stats = {
      totalEventsEmitted: 0,
      eventsByType: new Map(),
    };
  }
}
```

- [ ] **Step 4: Create index.ts export**

```typescript
// src/utils/events/index.ts
export { EventBus } from './EventBus.js';
export type * from '../../types/events.js';
```

- [ ] **Step 5: Run test to verify it passes**

```bash
bun test src/utils/events/EventBus.test.ts -v
```
Expected: PASS all 4 tests

- [ ] **Step 6: Commit**

```bash
git add src/utils/events/ src/types/events.ts
git commit -m "feat: implement event bus for decoupled communication"
```

---

### Task 3: Implement Lazy Loading Infrastructure

**Goal:** Create lazy loading utilities for tools, commands, and services

**Files:**
- Create: `src/utils/lazy/LazyRegistry.ts`
- Create: `src/utils/lazy/index.ts`
- Test: `src/utils/lazy/LazyRegistry.test.ts`
- Modify: `src/tools.ts` (add lazy loading)

- [ ] **Step 1: Write failing tests**

```typescript
// src/utils/lazy/LazyRegistry.test.ts
import { describe, test, expect, beforeEach } from 'bun:test';
import { LazyRegistry } from './LazyRegistry.js';

describe('LazyRegistry', () => {
  let registry: LazyRegistry<string>;

  beforeEach(() => {
    registry = new LazyRegistry();
  });

  test('should lazy load items on first access', async () => {
    let loadCount = 0;

    registry.register('item1', async () => {
      loadCount++;
      return 'loaded-item1';
    });

    expect(loadCount).toBe(0);

    const result = await registry.get('item1');

    expect(result).toBe('loaded-item1');
    expect(loadCount).toBe(1);
  });

  test('should cache loaded items', async () => {
    let loadCount = 0;

    registry.register('item1', async () => {
      loadCount++;
      return 'loaded-item1';
    });

    await registry.get('item1');
    await registry.get('item1');
    await registry.get('item1');

    expect(loadCount).toBe(1);
  });

  test('should return undefined for unregistered items', async () => {
    const result = await registry.get('nonexistent');
    expect(result).toBeUndefined();
  });

  test('should list registered keys without loading', async () => {
    registry.register('a', async () => 'a');
    registry.register('b', async () => 'b');
    registry.register('c', async () => 'c');

    const keys = registry.keys();
    expect(keys).toEqual(['a', 'b', 'c']);
  });

  test('should support bulk loading', async () => {
    const loadOrder: string[] = [];

    registry.register('a', async () => {
      loadOrder.push('a');
      return 'a';
    });
    registry.register('b', async () => {
      loadOrder.push('b');
      return 'b';
    });

    await registry.loadAll(['b', 'a']);

    expect(loadOrder).toEqual(['b', 'a']);
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

```bash
bun test src/utils/lazy/LazyRegistry.test.ts
```
Expected: FAIL - module not found

- [ ] **Step 3: Implement LazyRegistry**

```typescript
// src/utils/lazy/LazyRegistry.ts
type Loader<T> = () => Promise<T>;

interface LazyEntry<T> {
  loader: Loader<T>;
  loaded: boolean;
  value: T | null;
  error: Error | null;
}

export class LazyRegistry<T> {
  private entries: Map<string, LazyEntry<T>> = new Map();

  register(key: string, loader: Loader<T>): void {
    this.entries.set(key, {
      loader,
      loaded: false,
      value: null,
      error: null,
    });
  }

  async get(key: string): Promise<T | undefined> {
    const entry = this.entries.get(key);
    if (!entry) {
      return undefined;
    }

    if (entry.loaded) {
      return entry.error ? undefined : (entry.value as T);
    }

    try {
      entry.value = await entry.loader();
      entry.loaded = true;
      return entry.value;
    } catch (err) {
      entry.error = err as Error;
      throw err;
    }
  }

  async loadAll(keys: string[]): Promise<Map<string, T>> {
    const results = new Map<string, T>();

    await Promise.all(
      keys.map(async (key) => {
        const value = await this.get(key);
        if (value !== undefined) {
          results.set(key, value);
        }
      })
    );

    return results;
  }

  keys(): string[] {
    return Array.from(this.entries.keys());
  }

  isLoaded(key: string): boolean {
    return this.entries.get(key)?.loaded ?? false;
  }

  clear(): void {
    this.entries.clear();
  }
}
```

- [ ] **Step 4: Create index.ts export**

```typescript
// src/utils/lazy/index.ts
export { LazyRegistry } from './LazyRegistry.js';
```

- [ ] **Step 5: Run test to verify it passes**

```bash
bun test src/utils/lazy/LazyRegistry.test.ts -v
```
Expected: PASS all 5 tests

- [ ] **Step 6: Commit**

```bash
git add src/utils/lazy/
git commit -m "feat: implement lazy loading registry"
```

---

### Task 4: Fix LazyRegistry Race Condition

**Goal:** Prevent duplicate loads when concurrent requests hit same key

**Files:**
- Modify: `src/utils/lazy/LazyRegistry.ts`

- [ ] **Step 1: Write race condition test**

```typescript
// Add to src/utils/lazy/LazyRegistry.test.ts
test('should share pending promises for concurrent requests', async () => {
  const registry = new LazyRegistry();
  let loadCount = 0;

  registry.register('slow', async () => {
    loadCount++;
    await new Promise(resolve => setTimeout(resolve, 100));
    return 'loaded';
  });

  // Fire 3 concurrent requests
  const [result1, result2, result3] = await Promise.all([
    registry.get('slow'),
    registry.get('slow'),
    registry.get('slow'),
  ]);

  expect(loadCount).toBe(1); // Should only load once
  expect(result1).toBe('loaded');
  expect(result2).toBe('loaded');
  expect(result3).toBe('loaded');
});
```

- [ ] **Step 2: Run test to verify it fails**

```bash
bun test src/utils/lazy/LazyRegistry.test.ts -v
```
Expected: FAIL - loadCount will be 3 instead of 1

- [ ] **Step 3: Fix LazyRegistry with promise tracking**

```typescript
// src/utils/lazy/LazyRegistry.ts - UPDATED
type Loader<T> = () => Promise<T>;

interface LazyEntry<T> {
  loader: Loader<T>;
  loaded: boolean;
  value: T | null;
  error: Error | null;
  pendingPromise: Promise<T> | null; // NEW: Track pending load
}

export class LazyRegistry<T> {
  private entries: Map<string, LazyEntry<T>> = new Map();

  register(key: string, loader: Loader<T>): void {
    this.entries.set(key, {
      loader,
      loaded: false,
      value: null,
      error: null,
      pendingPromise: null, // NEW
    });
  }

  async get(key: string): Promise<T | undefined> {
    const entry = this.entries.get(key);
    if (!entry) {
      return undefined;
    }

    if (entry.loaded) {
      return entry.error ? undefined : (entry.value as T);
    }

    // NEW: If already loading, return same promise
    if (entry.pendingPromise) {
      return entry.pendingPromise;
    }

    // Start loading and track the promise
    entry.pendingPromise = (async () => {
      try {
        entry.value = await entry.loader();
        entry.loaded = true;
        return entry.value;
      } catch (err) {
        entry.error = err as Error;
        throw err;
      } finally {
        entry.pendingPromise = null; // Clear when done
      }
    })();

    return entry.pendingPromise;
  }

  // ... rest of methods unchanged
}
```

- [ ] **Step 4: Run test to verify it passes**

```bash
bun test src/utils/lazy/LazyRegistry.test.ts -v
```
Expected: PASS all 6 tests

- [ ] **Step 5: Commit**

```bash
git add src/utils/lazy/LazyRegistry.ts
git commit -m "fix: prevent duplicate loads with promise tracking"
```

---

### Task 5: Convert Tools to Lazy Loading

**Goal:** Modify tools.ts to use lazy loading for all tools

**Files:**
- Modify: `src/tools.ts` (complete rewrite of tool assembly)
- Modify: `src/Tool.ts` (add lazy loading types)

- [ ] **Step 1: Read current tools.ts to understand structure**

```bash
wc -l src/tools.ts
head -150 src/tools.ts
```

- [ ] **Step 2: Create lazy tools registry file**

```typescript
// src/tools/lazyRegistry.ts - NEW FILE
import { LazyRegistry } from '../utils/lazy/LazyRegistry.js';
import type { Tool } from '../Tool.js';

export const toolRegistry = new LazyRegistry<Tool>();

// Register all tools lazily
export function registerTools(): void {
  // Core tools
  toolRegistry.register('BashTool', () =>
    import('./BashTool/BashTool.js').then(m => m.BashTool)
  );
  toolRegistry.register('FileReadTool', () =>
    import('./FileReadTool/FileReadTool.js').then(m => m.FileReadTool)
  );
  toolRegistry.register('FileEditTool', () =>
    import('./FileEditTool/FileEditTool.js').then(m => m.FileEditTool)
  );
  toolRegistry.register('FileWriteTool', () =>
    import('./FileWriteTool/FileWriteTool.js').then(m => m.FileWriteTool)
  );

  // Search tools
  toolRegistry.register('GlobTool', () =>
    import('./GlobTool/GlobTool.js').then(m => m.GlobTool)
  );
  toolRegistry.register('GrepTool', () =>
    import('./GrepTool/GrepTool.js').then(m => m.GrepTool)
  );

  // Agent tools
  toolRegistry.register('AgentTool', () =>
    import('./AgentTool/AgentTool.js').then(m => m.AgentTool)
  );
  toolRegistry.register('SkillTool', () =>
    import('./SkillTool/SkillTool.js').then(m => m.SkillTool)
  );

  // Task tools
  toolRegistry.register('TaskCreateTool', () =>
    import('./TaskCreateTool/TaskCreateTool.js').then(m => m.TaskCreateTool)
  );
  toolRegistry.register('TaskListTool', () =>
    import('./TaskListTool/TaskListTool.js').then(m => m.TaskListTool)
  );

  // Web tools
  toolRegistry.register('WebSearchTool', () =>
    import('./WebSearchTool/WebSearchTool.js').then(m => m.WebSearchTool)
  );
  toolRegistry.register('WebFetchTool', () =>
    import('./WebFetchTool/WebFetchTool.js').then(m => m.WebFetchTool)
  );

  // MCP tools
  toolRegistry.register('ListMcpResourcesTool', () =>
    import('./ListMcpResourcesTool/ListMcpResourcesTool.js').then(m => m.ListMcpResourcesTool)
  );
  toolRegistry.register('ReadMcpResourceTool', () =>
    import('./ReadMcpResourceTool/ReadMcpResourceTool.js').then(m => m.ReadMcpResourceTool)
  );

  // Add all remaining tools following same pattern...
}

// Helper to get a single tool
export async function getTool(name: string): Promise<Tool | undefined> {
  return toolRegistry.get(name);
}

// Helper to get all tools
export async function getAllTools(): Promise<Tool[]> {
  const allTools = await toolRegistry.loadAll(toolRegistry.keys());
  return Array.from(allTools.values());
}
```

- [ ] **Step 3: Modify main tools.ts to use lazy registry**

```typescript
// src/tools.ts - Modify existing file
import { toolRegistry, registerTools, getTool, getAllTools } from './lazyRegistry.js';

// Initialize tool registration
registerTools();

// Export lazy functions
export { getTool, getAllTools, toolRegistry };

// Keep backward compatibility with async wrapper
export async function getTools(permissionContext: ToolPermissionContext): Promise<Tools> {
  if (isEnvTruthy(process.env.CLAUDE_CODE_SIMPLE)) {
    const [bashTool, fileReadTool, fileEditTool] = await Promise.all([
      toolRegistry.get('BashTool'),
      toolRegistry.get('FileReadTool'),
      toolRegistry.get('FileEditTool'),
    ]);
    return [bashTool, fileReadTool, fileEditTool].filter(Boolean) as Tools;
  }

  return getAllTools();
}
```

- [ ] **Step 4: Update query.ts to use async getTools**

```typescript
// In src/query.ts - find and update getTools calls
// Example change:

// BEFORE:
const tools = getTools(permissionContext);

// AFTER:
const tools = await getTools(permissionContext);
```

- [ ] **Step 5: Run typecheck**

```bash
bun run typecheck
```
Expected: Fix any async/await type errors

- [ ] **Step 6: Run tools tests**

```bash
bun test src/tools/ -v
```

- [ ] **Step 7: Commit**

```bash
git add src/tools.ts src/tools/lazyRegistry.ts src/Tool.ts
git commit -m "feat: convert tools to lazy loading with async registry"
```

---

### Task 6: Implement Caching Service - Core

**Goal:** Create multi-tier caching service with memory and disk caches

**Files:**
- Create: `src/services/cache/CacheService.ts`
- Create: `src/services/cache/MemoryCache.ts`
- Create: `src/services/cache/DiskCache.ts`
- Create: `src/services/cache/index.ts`
- Test: `src/services/cache/CacheService.test.ts`
- Test: `src/services/cache/MemoryCache.test.ts`

- [ ] **Step 1: Write MemoryCache tests**

```typescript
// src/services/cache/MemoryCache.test.ts
import { describe, test, expect, beforeEach } from 'bun:test';
import { MemoryCache } from './MemoryCache.js';

describe('MemoryCache', () => {
  let cache: MemoryCache;

  beforeEach(() => {
    cache = new MemoryCache({ maxSize: 100, ttlMs: 5000 });
  });

  test('should store and retrieve values', () => {
    cache.set('key1', 'value1');
    expect(cache.get('key1')).toBe('value1');
  });

  test('should return undefined for missing keys', () => {
    expect(cache.get('nonexistent')).toBeUndefined();
  });

  test('should respect LRU eviction', () => {
    const smallCache = new MemoryCache({ maxSize: 3, ttlMs: 60000 });

    smallCache.set('a', '1');
    smallCache.set('b', '2');
    smallCache.set('c', '3');
    smallCache.set('d', '4'); // Should evict 'a'

    expect(smallCache.get('a')).toBeUndefined();
    expect(smallCache.get('b')).toBe('2');
    expect(smallCache.get('c')).toBe('3');
    expect(smallCache.get('d')).toBe('4');
  });

  test('should update LRU order on access', () => {
    const smallCache = new MemoryCache({ maxSize: 3, ttlMs: 60000 });

    smallCache.set('a', '1');
    smallCache.set('b', '2');
    smallCache.set('c', '3');
    smallCache.get('a'); // Access 'a' to make it recently used
    smallCache.set('d', '4'); // Should evict 'b' (least recently used)

    expect(smallCache.get('a')).toBe('1');
    expect(smallCache.get('b')).toBeUndefined();
    expect(smallCache.get('c')).toBe('3');
    expect(smallCache.get('d')).toBe('4');
  });

  test('should expire entries after TTL', async () => {
    const shortTtlCache = new MemoryCache({ maxSize: 10, ttlMs: 50 });

    shortTtlCache.set('key', 'value');
    expect(shortTtlCache.get('key')).toBe('value');

    await new Promise(resolve => setTimeout(resolve, 60));

    expect(shortTtlCache.get('key')).toBeUndefined();
  });

  test('should clear all entries', () => {
    cache.set('a', '1');
    cache.set('b', '2');
    cache.clear();

    expect(cache.get('a')).toBeUndefined();
    expect(cache.get('b')).toBeUndefined();
  });

  test('should report statistics', () => {
    cache.set('a', '1');
    cache.set('b', '2');
    cache.set('c', '3');

    const stats = cache.getStats();
    expect(stats.size).toBe(3);
    expect(stats.hits).toBe(0);
    expect(stats.misses).toBe(0);

    cache.get('a');
    cache.get('a');
    cache.get('nonexistent');

    const updatedStats = cache.getStats();
    expect(updatedStats.hits).toBe(2);
    expect(updatedStats.misses).toBe(1);
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

```bash
bun test src/services/cache/MemoryCache.test.ts
```
Expected: FAIL - module not found

- [ ] **Step 3: Implement MemoryCache**

```typescript
// src/services/cache/MemoryCache.ts
interface CacheEntry<T> {
  value: T;
  expiresAt: number;
}

interface LRUNode<T> {
  key: string;
  value: CacheEntry<T>;
  prev: LRUNode<T> | null;
  next: LRUNode<T> | null;
}

interface MemoryCacheOptions {
  maxSize: number;
  ttlMs: number;
}

interface CacheStats {
  size: number;
  hits: number;
  misses: number;
  evictions: number;
}

export class MemoryCache<T = unknown> {
  private entries: Map<string, LRUNode<T>> = new Map();
  private head: LRUNode<T> | null = null;
  private tail: LRUNode<T> | null = null;
  private readonly maxSize: number;
  private readonly ttlMs: number;
  private stats: CacheStats = {
    size: 0,
    hits: 0,
    misses: 0,
    evictions: 0,
  };

  constructor(options: MemoryCacheOptions) {
    this.maxSize = options.maxSize;
    this.ttlMs = options.ttlMs;
  }

  set(key: string, value: T): void {
    // Remove existing entry if present
    const existing = this.entries.get(key);
    if (existing) {
      this.removeNode(existing);
    }

    // Create new entry
    const entry: CacheEntry<T> = {
      value,
      expiresAt: Date.now() + this.ttlMs,
    };

    const node: LRUNode<T> = {
      key,
      value: entry,
      prev: null,
      next: this.head,
    };

    // Add to front of LRU list
    if (this.head) {
      this.head.prev = node;
    }
    this.head = node;

    if (!this.tail) {
      this.tail = node;
    }

    this.entries.set(key, node);
    this.stats.size = this.entries.size;

    // Evict if over capacity
    while (this.entries.size > this.maxSize && this.tail) {
      this.removeNode(this.tail);
      this.stats.evictions++;
    }
  }

  get(key: string): T | undefined {
    const node = this.entries.get(key);

    if (!node) {
      this.stats.misses++;
      return undefined;
    }

    // Check expiration
    if (Date.now() > node.value.expiresAt) {
      this.removeNode(node);
      this.stats.misses++;
      return undefined;
    }

    // Move to front (most recently used)
    this.removeNode(node);
    node.prev = null;
    node.next = this.head;
    if (this.head) {
      this.head.prev = node;
    }
    this.head = node;
    if (!this.tail) {
      this.tail = node;
    }

    this.stats.hits++;
    return node.value.value;
  }

  has(key: string): boolean {
    return this.get(key) !== undefined;
  }

  delete(key: string): boolean {
    const node = this.entries.get(key);
    if (!node) {
      return false;
    }
    this.removeNode(node);
    return true;
  }

  clear(): void {
    this.entries.clear();
    this.head = null;
    this.tail = null;
    this.stats = { size: 0, hits: 0, misses: 0, evictions: 0 };
  }

  getStats(): CacheStats {
    return { ...this.stats };
  }

  private removeNode(node: LRUNode<T>): void {
    this.entries.delete(node.key);

    if (node.prev) {
      node.prev.next = node.next;
    } else {
      this.head = node.next;
    }

    if (node.next) {
      node.next.prev = node.prev;
    } else {
      this.tail = node.prev;
    }

    this.stats.size = this.entries.size;
  }
}
```

- [ ] **Step 4: Run test to verify it passes**

```bash
bun test src/services/cache/MemoryCache.test.ts -v
```
Expected: PASS all 7 tests

- [ ] **Step 5: Commit**

```bash
git add src/services/cache/MemoryCache.ts src/services/cache/MemoryCache.test.ts
git commit -m "feat: implement LRU memory cache with TTL"
```

---

### Task 7: Implement Caching Service - Disk Cache

**Goal:** Implement SQLite-based disk cache for persistent caching

**Files:**
- Create: `src/services/cache/DiskCache.ts`
- Test: `src/services/cache/DiskCache.test.ts`

- [ ] **Step 1: Write DiskCache tests**

```typescript
// src/services/cache/DiskCache.test.ts
import { describe, test, expect, beforeEach, afterAll } from 'bun:test';
import { DiskCache } from './DiskCache.js';
import { mkdtemp, rm } from 'fs/promises';
import { tmpdir } from 'os';
import { join } from 'path';

describe('DiskCache', () => {
  let tempDir: string;
  let cache: DiskCache;

  beforeEach(async () => {
    tempDir = await mkdtemp(join(tmpdir(), 'diskcache-test-'));
    cache = new DiskCache({ dbPath: join(tempDir, 'cache.db'), ttlMs: 5000 });
    await cache.initialize();
  });

  afterAll(async () => {
    await cache.dispose();
    await rm(tempDir, { recursive: true, force: true });
  });

  test('should store and retrieve values', async () => {
    await cache.set('key1', 'value1');
    const result = await cache.get('key1');
    expect(result).toBe('value1');
  });

  test('should return undefined for missing keys', async () => {
    const result = await cache.get('nonexistent');
    expect(result).toBeUndefined();
  });

  test('should handle complex objects', async () => {
    const complexValue = {
      nested: { data: [1, 2, 3] },
      timestamp: Date.now(),
    };

    await cache.set('complex', complexValue);
    const result = await cache.get('complex');
    expect(result).toEqual(complexValue);
  });

  test('should expire entries after TTL', async () => {
    await cache.set('expiring', 'value', 50);
    expect(await cache.get('expiring')).toBe('value');

    await new Promise(resolve => setTimeout(resolve, 60));

    expect(await cache.get('expiring')).toBeUndefined();
  });

  test('should delete entries', async () => {
    await cache.set('toDelete', 'value');
    await cache.delete('toDelete');
    expect(await cache.get('toDelete')).toBeUndefined();
  });

  test('should clear all entries', async () => {
    await cache.set('a', '1');
    await cache.set('b', '2');
    await cache.set('c', '3');

    await cache.clear();

    expect(await cache.get('a')).toBeUndefined();
    expect(await cache.get('b')).toBeUndefined();
    expect(await cache.get('c')).toBeUndefined();
  });
});
```

- [ ] **Step 2: Implement DiskCache**

```typescript
// src/services/cache/DiskCache.ts
import { Database } from 'bun:sqlite';
import type { Service, HealthStatus } from '../../types/core.js';

interface DiskCacheOptions {
  dbPath: string;
  ttlMs?: number;
}

export class DiskCache implements Service {
  private db: Database | null = null;
  private readonly dbPath: string;
  private readonly ttlMs: number;
  private initialized = false;

  constructor(options: DiskCacheOptions) {
    this.dbPath = options.dbPath;
    this.ttlMs = options.ttlMs ?? 3600000; // 1 hour default
  }

  async initialize(): Promise<void> {
    if (this.initialized) return;

    this.db = new Database(this.dbPath);

    // Create cache table
    this.db.exec(`
      CREATE TABLE IF NOT EXISTS cache (
        key TEXT PRIMARY KEY,
        value TEXT NOT NULL,
        expiresAt INTEGER NOT NULL
      )
    `);

    // Create index for expiration queries
    this.db.exec(`
      CREATE INDEX IF NOT EXISTS idx_expires ON cache(expiresAt)
    `);

    this.initialized = true;
  }

  async set(key: string, value: unknown, ttlMs?: number): Promise<void> {
    if (!this.initialized) await this.initialize();

    const expiresAt = Date.now() + (ttlMs ?? this.ttlMs);
    const valueJson = JSON.stringify(value);

    this.db!.exec(
      'INSERT OR REPLACE INTO cache (key, value, expiresAt) VALUES (?, ?, ?)',
      key,
      valueJson,
      expiresAt
    );
  }

  async get<T>(key: string): Promise<T | undefined> {
    if (!this.initialized) await this.initialize();

    const row = this.db!.query('SELECT value, expiresAt FROM cache WHERE key = ?').get(key) as
      | { value: string; expiresAt: number }
      | undefined;

    if (!row) {
      return undefined;
    }

    // Check expiration
    if (Date.now() > row.expiresAt) {
      await this.delete(key);
      return undefined;
    }

    return JSON.parse(row.value) as T;
  }

  async has(key: string): Promise<boolean> {
    const value = await this.get(key);
    return value !== undefined;
  }

  async delete(key: string): Promise<void> {
    if (!this.initialized) await this.initialize();

    this.db!.exec('DELETE FROM cache WHERE key = ?', key);
  }

  async clear(): Promise<void> {
    if (!this.initialized) await this.initialize();

    this.db!.exec('DELETE FROM cache');
  }

  async dispose(): Promise<void> {
    if (this.db) {
      this.db.close();
      this.db = null;
      this.initialized = false;
    }
  }

  async healthCheck(): Promise<HealthStatus> {
    const checks = new Map();

    try {
      if (!this.initialized) {
        await this.initialize();
      }

      // Test read/write
      const testKey = `_health_${Date.now()}`;
      await this.set(testKey, 'health');
      const result = await this.get(testKey);
      await this.delete(testKey);

      checks.set('database', { ok: result === 'health' });
      checks.set('initialized', { ok: this.initialized });
    } catch (err) {
      checks.set('database', { ok: false, message: (err as Error).message });
    }

    return {
      healthy: Array.from(checks.values()).every(c => c.ok),
      checks,
      lastCheck: Date.now(),
    };
  }

  get name(): string {
    return 'DiskCache';
  }
}
```

- [ ] **Step 3: Run tests**

```bash
bun test src/services/cache/DiskCache.test.ts -v
```

- [ ] **Step 4: Commit**

```bash
git add src/services/cache/DiskCache.ts src/services/cache/DiskCache.test.ts
git commit -m "feat: implement SQLite disk cache"
```

---

### Task 8: Implement Caching Service - Main Service

**Goal:** Combine memory and disk cache into unified CacheService

**Files:**
- Create: `src/services/cache/CacheService.ts` (main service)
- Create: `src/services/cache/types.ts`
- Modify: `src/services/cache/index.ts` (export service)

- [ ] **Step 1: Create cache types**

```typescript
// src/services/cache/types.ts
import type { Service } from '../../types/core.js';

export interface CacheServiceConfig {
  memoryMaxSize: number;
  memoryTtlMs: number;
  diskEnabled: boolean;
  diskPath?: string;
  diskTtlMs: number;
}

export interface CacheStats {
  memory: {
    size: number;
    hits: number;
    misses: number;
    evictions: number;
  };
  disk?: {
    size: number;
    hits: number;
    misses: number;
  };
  totalHits: number;
  totalMisses: number;
  hitRate: number;
}

export interface CacheLayer {
  get<T>(key: string): Promise<T | undefined> | T | undefined;
  set(key: string, value: unknown, ttlMs?: number): Promise<void> | void;
  delete(key: string): Promise<void> | void;
  clear(): Promise<void> | void;
  has(key: string): Promise<boolean> | boolean;
}
```

- [ ] **Step 2: Implement CacheService**

```typescript
// src/services/cache/CacheService.ts
import type { Service, HealthStatus } from '../../types/core.js';
import { MemoryCache } from './MemoryCache.js';
import { DiskCache } from './DiskCache.js';
import type { CacheServiceConfig, CacheStats } from './types.js';

const DEFAULT_CONFIG: CacheServiceConfig = {
  memoryMaxSize: 1000,
  memoryTtlMs: 300000, // 5 minutes
  diskEnabled: true,
  diskTtlMs: 3600000, // 1 hour
};

export class CacheService implements Service {
  private memoryCache: MemoryCache;
  private diskCache: DiskCache | null = null;
  private config: CacheServiceConfig;
  private stats = {
    memoryHits: 0,
    memoryMisses: 0,
    diskHits: 0,
    diskMisses: 0,
  };

  constructor(config: Partial<CacheServiceConfig> = {}) {
    this.config = { ...DEFAULT_CONFIG, ...config };
    this.memoryCache = new MemoryCache({
      maxSize: this.config.memoryMaxSize,
      ttlMs: this.config.memoryTtlMs,
    });

    if (this.config.diskEnabled && this.config.diskPath) {
      this.diskCache = new DiskCache({
        dbPath: this.config.diskPath,
        ttlMs: this.config.diskTtlMs,
      });
    }
  }

  async initialize(): Promise<void> {
    if (this.diskCache) {
      await this.diskCache.initialize();
    }
  }

  async get<T>(key: string): Promise<T | undefined> {
    // Try memory first
    const memoryResult = this.memoryCache.get(key);
    if (memoryResult !== undefined) {
      this.stats.memoryHits++;
      return memoryResult as T;
    }

    this.stats.memoryMisses++;

    // Try disk
    if (this.diskCache) {
      const diskResult = await this.diskCache.get<T>(key);
      if (diskResult !== undefined) {
        this.stats.diskHits++;
        // Populate memory cache
        this.memoryCache.set(key, diskResult);
        return diskResult;
      }
      this.stats.diskMisses++;
    }

    return undefined;
  }

  async set(key: string, value: unknown, ttlMs?: number): Promise<void> {
    // Always set in memory
    this.memoryCache.set(key, value);

    // Also set in disk if enabled
    if (this.diskCache) {
      await this.diskCache.set(key, value, ttlMs);
    }
  }

  async delete(key: string): Promise<void> {
    this.memoryCache.delete(key);

    if (this.diskCache) {
      await this.diskCache.delete(key);
    }
  }

  async clear(): Promise<void> {
    this.memoryCache.clear();

    if (this.diskCache) {
      await this.diskCache.clear();
    }
  }

  getStats(): CacheStats {
    const memoryStats = this.memoryCache.getStats();
    const totalHits = this.stats.memoryHits + this.stats.diskHits;
    const totalMisses = this.stats.memoryMisses + this.stats.diskMisses;
    const total = totalHits + totalMisses;

    return {
      memory: {
        size: memoryStats.size,
        hits: memoryStats.hits,
        misses: memoryStats.misses,
        evictions: memoryStats.evictions,
      },
      disk: this.diskCache ? {
        size: 0, // Would need to query SQLite
        hits: this.stats.diskHits,
        misses: this.stats.diskMisses,
      } : undefined,
      totalHits,
      totalMisses,
      hitRate: total > 0 ? totalHits / total : 0,
    };
  }

  async dispose(): Promise<void> {
    if (this.diskCache) {
      await this.diskCache.dispose();
      this.diskCache = null;
    }
    this.memoryCache.clear();
  }

  async healthCheck(): Promise<HealthStatus> {
    const checks = new Map();

    // Memory cache health
    try {
      this.memoryCache.set('_health', 'test');
      const result = this.memoryCache.get('_health');
      this.memoryCache.delete('_health');
      checks.set('memory', { ok: result === 'test' });
    } catch (err) {
      checks.set('memory', { ok: false, message: (err as Error).message });
    }

    // Disk cache health
    if (this.diskCache) {
      const diskHealth = await this.diskCache.healthCheck();
      checks.set('disk', diskHealth.checks.get('database') || { ok: false });
    }

    return {
      healthy: Array.from(checks.values()).every(c => c.ok),
      checks,
      lastCheck: Date.now(),
    };
  }

  get name(): string {
    return 'CacheService';
  }
}
```

- [ ] **Step 3: Update index.ts exports**

```typescript
// src/services/cache/index.ts
export { CacheService } from './CacheService.js';
export { MemoryCache } from './MemoryCache.js';
export { DiskCache } from './DiskCache.js';
export type * from './types.js';
```

- [ ] **Step 4: Write and run CacheService tests**

```bash
bun test src/services/cache/CacheService.test.ts -v
```

- [ ] **Step 5: Commit**

```bash
git add src/services/cache/
git commit -m "feat: implement unified cache service with memory and disk tiers"
```

---

## Review and Gotchas

### Critical Gotchas (Must Address Before Implementation)

1. **Circular Import Risk - CRITICAL**
   - The event bus must NOT import from modules that import from it
   - Keep `src/types/core.ts` completely isolated - NO imports from `src/`
   - Test: `grep -r "from '../" src/types/` should return nothing

2. **Async Migration Cascade**
   - Converting `getTools()` to async breaks the entire call chain
   - Must update ALL callers or app will hang on unresolved promises
   - Run `grep -r "getTools(" src/ --include="*.ts"` before starting Task 5
   - Estimated 20-30 files need updates

3. **Bun-Specific APIs**
   - `bun:sqlite` used in DiskCache won't work in Node
   - Solution: Task 11 adds adapter pattern
   - If deploying to Node, complete Task 11 before Task 7

4. **Memory Leak Risk in EventBus**
   - Every `subscribe()` MUST have matching `unsubscribe()`
   - Add subscription tracking: `const subscriptions = new Set<EventSubscription>()`
   - Clean up in component unmount / service dispose

5. **Cache Stampede (Race Condition)**
   - Fixed in Task 4 with promise tracking
   - DO NOT skip Task 4 or concurrent tool loads will multiply

6. **Cache Invalidation Gap**
   - File read caches become stale when files change
   - Task 9 adds chokidar-based invalidation
   - Until then: caches work but may serve stale data

### Testing Checklist Before Each Commit

```bash
# After Task 1-2 (Types + EventBus)
bun run typecheck
bun test src/types/ src/utils/events/

# After Task 3-4 (LazyRegistry + race fix)
bun test src/utils/lazy/ -v

# After Task 5 (Tools lazy loading)
bun run typecheck  # Expect errors, fix them
bun test src/tools/ --timeout=30000

# After Task 6-8 (Cache service)
bun test src/services/cache/ -v

# Full regression before merging
bun run build
bun run test:coverage
bun run smoke
```

### Rollback Plan

If lazy loading causes issues:
```bash
# Revert tools.ts changes
git checkout HEAD~1 -- src/tools.ts src/tools/lazyRegistry.ts

# Keep types and EventBus (they're useful independently)
# Re-implement lazy loading in separate branch
```

### Performance Benchmarks to Track

| Metric | Before | Target | Measure Command |
|--------|--------|--------|-----------------|
| Cold Start | ~500ms | <300ms | `time node dist/cli.mjs --version` |
| Memory (idle) | ~200MB | <150MB | Activity Monitor / `ps` |
| First Tool Load | ~50ms | <20ms | Add timing in LazyRegistry.get() |
| Cache Hit Rate | N/A | >80% | CacheService.getStats() |

### Additional Tasks for Complete Phase 1

**Task 9: Add File Watcher Cache Invalidation**
- Create `src/services/cache/FileWatcherCache.ts`
- Integrate with chokidar for file change detection
- Auto-invalidate file read caches when files change

**Task 10: Update All getTools() Callers**
- Systematic async migration across:
  - `src/query.ts` - main query loop
  - `src/repl/REPL.tsx` - REPL tool execution
  - `src/commands/*.ts` - command handlers
  - `src/tools/**/*.test.ts` - all tool tests

**Task 11: Add Node SQLite Fallback**
- Create `src/services/cache/SqliteAdapter.ts` with bun:sqlite and better-sqlite3 implementations
- Detect runtime (Bun vs Node) and use appropriate adapter

**Task 12: Event Bus Cleanup Integration**
- Add `dispose()` method to all EventBus subscribers
- Wire up unsubscribe in React component cleanup
- Add subscription tracking in service registry

---

## Phase 2-4 Summary (Detailed tasks to be created in separate plans)

### Phase 2: Performance (Months 4-6)

**8 Major Tasks:**
1. Rust FFI setup and build integration
2. Rust BashTool implementation
3. Rust FileReadTool implementation
4. Prompt caching with Anthropic API
5. Parallel tool execution engine
6. Rate limiting service
7. Health monitoring service
8. Update service

### Phase 3: Features (Months 7-9)

**10 Major Tasks:**
1. Plugin system core
2. Plugin sandboxing
3. Plugin marketplace UI
4. Vector store service
5. DatabaseTool
6. DockerTool
7. HttpTool
8. DiffTool
9. ProcessTool
10. Telemetry service (opt-in)

### Phase 4: Polish (Months 10-12)

**5 Major Tasks:**
1. Full Rust migration (optional)
2. Plugin marketplace launch
3. Advanced AI features
4. Enterprise features
5. Documentation overhaul

---

## Execution Choice

**Plan complete and saved to `docs/superpowers/plans/2026-04-13-architecture-improvements.md`. Two execution options:**

**1. Subagent-Driven (recommended)** - I dispatch a fresh subagent per task, review between tasks, fast iteration

**2. Inline Execution** - Execute tasks in this session using executing-plans, batch execution with checkpoints

**Which approach?**

Note: This plan is comprehensive but Phase 1 alone is 7+ tasks. Recommend starting with Tasks 1-3 (core types, event bus, lazy loading) as a minimal viable first iteration.
