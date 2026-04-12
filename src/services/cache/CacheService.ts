/**
 * Unified cache service combining memory and disk tiers.
 * Memory-first with disk persistence for cache resilience.
 */

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
    // Try memory first (fast path)
    const memoryResult = this.memoryCache.get(key);
    if (memoryResult !== undefined) {
      this.stats.memoryHits++;
      return memoryResult as T;
    }

    this.stats.memoryMisses++;

    // Try disk if enabled
    if (this.diskCache) {
      const diskResult = await this.diskCache.get<T>(key);
      if (diskResult !== undefined) {
        this.stats.diskHits++;
        // Populate memory cache for next access
        this.memoryCache.set(key, diskResult);
        return diskResult;
      }
      this.stats.diskMisses++;
    }

    return undefined;
  }

  async set(key: string, value: unknown, ttlMs?: number): Promise<void> {
    // Always set in memory (fast path)
    this.memoryCache.set(key, value);

    // Also set in disk if enabled (persistence)
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
