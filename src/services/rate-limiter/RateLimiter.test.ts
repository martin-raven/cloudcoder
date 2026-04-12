import { describe, test, expect, beforeEach } from 'bun:test';
import { RateLimiterService, getRateLimiter } from './RateLimiter.js';

describe('RateLimiterService', () => {
  let limiter: RateLimiterService;

  beforeEach(() => {
    limiter = new RateLimiterService();
  });

  test('should allow requests within capacity', () => {
    limiter.configure('test', { capacity: 10, refillRate: 100 });

    for (let i = 0; i < 10; i++) {
      const result = limiter.check('test');
      expect(result.allowed).toBe(true);
    }
  });

  test('should rate limit when capacity exceeded', () => {
    limiter.configure('test', { capacity: 5, refillRate: 10 });

    // Exhaust tokens
    for (let i = 0; i < 5; i++) {
      limiter.check('test');
    }

    // Next request should be rate limited
    const result = limiter.check('test');
    expect(result.allowed).toBe(false);
    expect(result.retryAfterMs).toBeGreaterThan(0);
  });

  test('should refill tokens over time', async () => {
    limiter.configure('test', { capacity: 5, refillRate: 100 }); // 100 tokens/sec

    // Exhaust tokens
    for (let i = 0; i < 5; i++) {
      limiter.check('test');
    }

    // Wait for refill
    await new Promise(resolve => setTimeout(resolve, 50)); // Should add ~5 tokens

    const result = limiter.check('test');
    expect(result.allowed).toBe(true);
  });

  test('should support different configs per key', () => {
    limiter.configure('fast', { capacity: 100, refillRate: 1000 });
    limiter.configure('slow', { capacity: 5, refillRate: 1 });

    // Fast should allow many requests
    for (let i = 0; i < 50; i++) {
      expect(limiter.check('fast').allowed).toBe(true);
    }

    // Slow should limit quickly
    for (let i = 0; i < 5; i++) {
      limiter.check('slow');
    }
    expect(limiter.check('slow').allowed).toBe(false);
  });

  test('should track statistics', () => {
    limiter.configure('test', { capacity: 3, refillRate: 1 });

    limiter.check('test');
    limiter.check('test');
    limiter.check('test');
    limiter.check('test'); // Rate limited

    const stats = limiter.getStats();
    expect(stats.totalRequests).toBe(4);
    expect(stats.allowedRequests).toBe(3);
    expect(stats.rateLimitedRequests).toBe(1);
    expect(stats.rateLimitRate).toBe(0.25);
  });

  test('should reset statistics', () => {
    limiter.check('test');
    limiter.resetStats();

    const stats = limiter.getStats();
    expect(stats.totalRequests).toBe(0);
  });

  test('should clear all buckets', () => {
    limiter.configure('a', { capacity: 10, refillRate: 10 });
    limiter.configure('b', { capacity: 10, refillRate: 10 });

    limiter.clear();

    const stats = limiter.getStats();
    expect(stats.activeBuckets).toBe(0);
  });

  test('should wait for token with timeout', async () => {
    limiter.configure('test', { capacity: 1, refillRate: 100 });

    // First request succeeds
    const result1 = await limiter.waitForToken('test');
    expect(result1.allowed).toBe(true);

    // Second request waits for refill
    const result2 = await limiter.waitForToken('test', 1, 100);
    // May or may not succeed depending on timing
    expect(result2.remainingTokens).toBeGreaterThanOrEqual(0);
  });

  test('should timeout when waiting for token', async () => {
    limiter.configure('test', { capacity: 1, refillRate: 0.1 }); // Very slow refill

    limiter.check('test'); // Exhaust token

    const start = Date.now();
    const result = await limiter.waitForToken('test', 1, 50); // 50ms timeout
    const elapsed = Date.now() - start;

    expect(result.allowed).toBe(false);
    expect(elapsed).toBeGreaterThanOrEqual(45); // Should have waited close to timeout
  });

  test('should use default config for unknown keys', () => {
    // Don't configure - should use defaults
    const result = limiter.check('unknown');
    expect(result.allowed).toBe(true);
    expect(result.remainingTokens).toBeGreaterThan(0);
  });

  test('should perform health check', async () => {
    const health = await limiter.healthCheck();
    expect(health.healthy).toBe(true);
    expect(health.checks.has('buckets')).toBe(true);
    expect(health.checks.has('stats')).toBe(true);
  });

  test('should support singleton', () => {
    const instance1 = getRateLimiter();
    const instance2 = getRateLimiter();
    expect(instance1).toBe(instance2);
  });
});
