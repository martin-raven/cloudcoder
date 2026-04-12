/**
 * Rate limiter service with token bucket algorithm.
 * Supports per-provider rate limiting with configurable buckets.
 */

import type { Service, HealthStatus } from '../../types/core.js';

interface RateLimitBucket {
  tokens: number;
  lastRefill: number;
  capacity: number;
  refillRate: number; // tokens per ms
}

interface RateLimitConfig {
  capacity: number;
  refillRate: number; // tokens per second
}

interface RateLimitResult {
  allowed: boolean;
  retryAfterMs?: number;
  remainingTokens: number;
}

const DEFAULT_CONFIG: RateLimitConfig = {
  capacity: 100,
  refillRate: 10, // 10 tokens per second
};

export class RateLimiterService implements Service {
  private buckets: Map<string, RateLimitBucket> = new Map();
  private configs: Map<string, RateLimitConfig> = new Map();
  private stats = {
    totalRequests: 0,
    allowedRequests: 0,
    rateLimitedRequests: 0,
  };

  /**
   * Configure rate limit for a specific key (e.g., provider name).
   */
  configure(key: string, config: RateLimitConfig): void {
    this.configs.set(key, config);

    // Update existing bucket if present
    const bucket = this.buckets.get(key);
    if (bucket) {
      bucket.capacity = config.capacity;
      bucket.refillRate = config.refillRate / 1000; // Convert to per-ms
      // Add tokens if new capacity is higher
      if (bucket.tokens < bucket.capacity) {
        bucket.tokens = Math.min(bucket.tokens + (config.capacity - bucket.capacity), config.capacity);
      }
    }
  }

  /**
   * Check if request is allowed and consume a token if so.
   */
  check(key: string, tokens: number = 1): RateLimitResult {
    this.stats.totalRequests++;

    const bucket = this.getOrCreateBucket(key);
    this.refillBucket(bucket);

    if (bucket.tokens >= tokens) {
      bucket.tokens -= tokens;
      this.stats.allowedRequests++;
      return {
        allowed: true,
        remainingTokens: Math.floor(bucket.tokens),
      };
    }

    this.stats.rateLimitedRequests++;

    // Calculate retry time
    const tokensNeeded = tokens - bucket.tokens;
    const retryAfterMs = Math.ceil(tokensNeeded / bucket.refillRate);

    return {
      allowed: false,
      retryAfterMs,
      remainingTokens: Math.floor(bucket.tokens),
    };
  }

  /**
   * Wait until request is allowed, then consume token.
   */
  async waitForToken(key: string, tokens: number = 1, timeoutMs: number = 30000): Promise<RateLimitResult> {
    const startTime = Date.now();

    while (Date.now() - startTime < timeoutMs) {
      const result = this.check(key, tokens);
      if (result.allowed) {
        return result;
      }

      if (result.retryAfterMs) {
        // Wait for tokens to refill, but cap at remaining timeout
        const waitTime = Math.min(result.retryAfterMs + 10, timeoutMs - (Date.now() - startTime));
        if (waitTime > 0) {
          await new Promise(resolve => setTimeout(resolve, waitTime));
        }
      }
    }

    // Timeout - return rate limited result
    return {
      allowed: false,
      remainingTokens: 0,
    };
  }

  /**
   * Get current token count for a key.
   */
  getTokens(key: string): number {
    const bucket = this.getOrCreateBucket(key);
    this.refillBucket(bucket);
    return Math.floor(bucket.tokens);
  }

  /**
   * Get statistics.
   */
  getStats(): {
    totalRequests: number;
    allowedRequests: number;
    rateLimitedRequests: number;
    rateLimitRate: number;
    activeBuckets: number;
  } {
    const rate = this.stats.totalRequests > 0
      ? this.stats.rateLimitedRequests / this.stats.totalRequests
      : 0;

    return {
      ...this.stats,
      rateLimitRate: rate,
      activeBuckets: this.buckets.size,
    };
  }

  /**
   * Reset statistics.
   */
  resetStats(): void {
    this.stats = {
      totalRequests: 0,
      allowedRequests: 0,
      rateLimitedRequests: 0,
    };
  }

  /**
   * Clear all buckets.
   */
  clear(): void {
    this.buckets.clear();
  }

  private getOrCreateBucket(key: string): RateLimitBucket {
    let bucket = this.buckets.get(key);

    if (!bucket) {
      const config = this.configs.get(key) ?? DEFAULT_CONFIG;
      bucket = {
        tokens: config.capacity,
        lastRefill: Date.now(),
        capacity: config.capacity,
        refillRate: config.refillRate / 1000, // Convert to per-ms
      };
      this.buckets.set(key, bucket);
    }

    return bucket;
  }

  private refillBucket(bucket: RateLimitBucket): void {
    const now = Date.now();
    const elapsed = now - bucket.lastRefill;
    const tokensToAdd = elapsed * bucket.refillRate;

    bucket.tokens = Math.min(bucket.capacity, bucket.tokens + tokensToAdd);
    bucket.lastRefill = now;
  }

  // Service interface
  async initialize(): Promise<void> {
    // Nothing to initialize
  }

  async dispose(): Promise<void> {
    this.clear();
  }

  async healthCheck(): Promise<HealthStatus> {
    const checks = new Map();
    checks.set('buckets', { ok: this.buckets.size >= 0 });
    checks.set('stats', { ok: this.stats.totalRequests >= 0 });

    return {
      healthy: true,
      checks,
      lastCheck: Date.now(),
    };
  }

  get name(): string {
    return 'RateLimiterService';
  }
}

// Singleton instance
let instance: RateLimiterService | null = null;

export function getRateLimiter(): RateLimiterService {
  if (!instance) {
    instance = new RateLimiterService();
  }
  return instance;
}

export type { RateLimitConfig, RateLimitResult };
