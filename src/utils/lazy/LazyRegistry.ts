/**
 * Lazy loading registry with race condition protection.
 * Prevents duplicate loads when concurrent requests hit the same key.
 */

import type { LazyLoader, LazyRegistryStats } from '../../types/core.js';

interface LazyEntry<T> {
  loader: LazyLoader<T> | null;
  loaded: boolean;
  value: T | null;
  error: Error | null;
  pendingPromise: Promise<T> | null; // Track pending load to prevent race conditions
}

export class LazyRegistry<T> {
  private entries: Map<string, LazyEntry<T>> = new Map();

  register(key: string, loader: LazyLoader<T>): void {
    this.entries.set(key, {
      loader,
      loaded: false,
      value: null,
      error: null,
      pendingPromise: null,
    });
  }

  async get(key: string): Promise<T | undefined> {
    const entry = this.entries.get(key);
    if (!entry) {
      return undefined;
    }

    // Already loaded successfully
    if (entry.loaded && entry.value !== null) {
      return entry.value;
    }

    // Already failed
    if (entry.loaded && entry.error !== null) {
      throw entry.error;
    }

    // Already loading - return same promise (race condition fix)
    if (entry.pendingPromise) {
      return entry.pendingPromise;
    }

    // Start loading and track the promise
    entry.pendingPromise = (async (): Promise<T> => {
      if (!entry.loader) {
        throw new Error(`No loader registered for key: ${key}`);
      }

      try {
        const value = await entry.loader();
        entry.value = value;
        entry.loaded = true;
        return value;
      } catch (err) {
        entry.error = err as Error;
        entry.loaded = true;
        throw err;
      } finally {
        entry.pendingPromise = null;
        entry.loader = null; // Allow GC of loader
      }
    })();

    return entry.pendingPromise;
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

  getStats(): LazyRegistryStats {
    let loadedKeys = 0;
    let pendingKeys = 0;
    let failedKeys = 0;

    for (const entry of this.entries.values()) {
      if (entry.loaded) {
        if (entry.error !== null) {
          failedKeys++;
        } else {
          loadedKeys++;
        }
      } else if (entry.pendingPromise !== null) {
        pendingKeys++;
      }
    }

    return {
      totalKeys: this.entries.size,
      loadedKeys,
      pendingKeys,
      failedKeys,
    };
  }

  clear(): void {
    this.entries.clear();
  }
}
