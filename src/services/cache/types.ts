/**
 * Cache service types.
 */

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
