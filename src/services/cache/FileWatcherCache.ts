/**
 * File watcher-based cache invalidation.
 * Automatically invalidates cached entries when files change.
 */

import type { CacheLayer } from './types.js';
import type { EventType } from '../../types/core.js';
import { EventBus } from '../../utils/events/EventBus.js';

interface FileWatcherCacheOptions {
  cache: CacheLayer;
  eventBus?: EventBus;
  debounceMs?: number;
}

interface WatchedFile {
  path: string;
  cacheKey: string;
  lastModified: number;
}

export class FileWatcherCache implements CacheLayer {
  private cache: CacheLayer;
  private eventBus: EventBus | null;
  private watchedFiles: Map<string, WatchedFile> = new Map();
  private readonly debounceMs: number;
  private debounceTimers: Map<string, NodeJS.Timeout> = new Map();

  constructor(options: FileWatcherCacheOptions) {
    this.cache = options.cache;
    this.eventBus = options.eventBus ?? null;
    this.debounceMs = options.debounceMs ?? 100;

    // Subscribe to file change events if event bus provided
    if (this.eventBus) {
      this.eventBus.subscribe('settings_change', (event) => {
        this.handleFileChange(event.payload as { path?: string });
      });
    }
  }

  /**
   * Watch a file for changes and invalidate cache key when modified.
   */
  watch(filePath: string, cacheKey: string): void {
    this.watchedFiles.set(filePath, {
      path: filePath,
      cacheKey,
      lastModified: Date.now(),
    });
  }

  /**
   * Stop watching a file.
   */
  unwatch(filePath: string): void {
    this.watchedFiles.delete(filePath);
    const timer = this.debounceTimers.get(filePath);
    if (timer) {
      clearTimeout(timer);
      this.debounceTimers.delete(filePath);
    }
  }

  /**
   * Handle file change event with debouncing.
   */
  private handleFileChange(payload: { path?: string }): void {
    if (!payload.path) return;

    const watched = this.watchedFiles.get(payload.path);
    if (!watched) return;

    // Debounce invalidation
    const existingTimer = this.debounceTimers.get(payload.path);
    if (existingTimer) {
      clearTimeout(existingTimer);
    }

    const timer = setTimeout(() => {
      // Invalidate the cache key
      void this.cache.delete(watched.cacheKey);
      this.debounceTimers.delete(payload.path);

      // Emit invalidation event
      if (this.eventBus) {
        this.eventBus.emit({
          type: 'settings_change',
          payload: {
            type: 'cache_invalidation',
            path: watched.path,
            cacheKey: watched.cacheKey,
          },
          timestamp: Date.now(),
          source: 'FileWatcherCache',
        });
      }
    }, this.debounceMs);

    this.debounceTimers.set(payload.path, timer);
  }

  /**
   * Manual invalidation by path.
   */
  invalidate(path: string): void {
    this.handleFileChange({ path });
  }

  /**
   * Clear all watched files.
   */
  clearWatched(): void {
    for (const timer of this.debounceTimers.values()) {
      clearTimeout(timer);
    }
    this.debounceTimers.clear();
    this.watchedFiles.clear();
  }

  // Delegate cache operations
  async get<T>(key: string): Promise<T | undefined> {
    return this.cache.get<T>(key);
  }

  async set(key: string, value: unknown, ttlMs?: number): Promise<void> {
    return this.cache.set(key, value, ttlMs);
  }

  async delete(key: string): Promise<void> {
    return this.cache.delete(key);
  }

  async clear(): Promise<void> {
    return this.cache.clear();
  }

  async has(key: string): Promise<boolean> {
    return this.cache.has(key);
  }

  /**
   * Get statistics about watched files.
   */
  getWatchStats(): { watchedCount: number; pendingInvalidations: number } {
    return {
      watchedCount: this.watchedFiles.size,
      pendingInvalidations: this.debounceTimers.size,
    };
  }
}
