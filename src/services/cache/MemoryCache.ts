/**
 * LRU memory cache with TTL support.
 */

import type { CacheStats, CacheOptions } from '../../types/core.js';

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

interface MemoryCacheStats {
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
  private stats: MemoryCacheStats = {
    size: 0,
    hits: 0,
    misses: 0,
    evictions: 0,
  };

  constructor(options: CacheOptions) {
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

    // Re-add to front of list
    node.prev = null;
    node.next = this.head;
    if (this.head) {
      this.head.prev = node;
    }
    this.head = node;
    if (!this.tail) {
      this.tail = node;
    }

    // Re-add to entries map
    this.entries.set(key, node);

    this.stats.hits++;
    return node.value.value;
  }

  has(key: string): boolean {
    const node = this.entries.get(key);
    if (!node) {
      return false;
    }
    // Check expiration without consuming a hit
    if (Date.now() > node.value.expiresAt) {
      return false;
    }
    return true;
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
    return { ...this.stats, hitRate: this.calculateHitRate() };
  }

  private calculateHitRate(): number {
    const total = this.stats.hits + this.stats.misses;
    return total > 0 ? this.stats.hits / total : 0;
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
