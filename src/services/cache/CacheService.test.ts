import { describe, test, expect, beforeEach, afterAll } from 'bun:test';
import { CacheService } from './CacheService.js';
import { mkdtemp, rm } from 'fs/promises';
import { tmpdir } from 'os';
import { join } from 'path';

describe('CacheService', () => {
  let tempDir: string;
  let cache: CacheService;

  beforeEach(async () => {
    tempDir = await mkdtemp(join(tmpdir(), 'cache-test-'));
    cache = new CacheService({
      memoryMaxSize: 100,
      memoryTtlMs: 5000,
      diskEnabled: true,
      diskPath: join(tempDir, 'cache.db'),
      diskTtlMs: 5000,
    });
    await cache.initialize();
  });

  afterAll(async () => {
    await cache.dispose();
    await rm(tempDir, { recursive: true, force: true });
  });

  test('should get from memory cache first', async () => {
    await cache.set('key1', 'value1');
    const result = await cache.get('key1');
    expect(result).toBe('value1');
  });

  test('should populate memory from disk on cache miss', async () => {
    // Set directly in disk (bypass memory by using new cache instance)
    const cache2 = new CacheService({
      memoryMaxSize: 100,
      memoryTtlMs: 1, // Very short TTL
      diskEnabled: true,
      diskPath: join(tempDir, 'cache2.db'),
      diskTtlMs: 60000,
    });
    await cache2.initialize();

    await cache2.set('persistent', 'data');
    await cache2.dispose();

    // New instance should get from disk
    const cache3 = new CacheService({
      memoryMaxSize: 100,
      memoryTtlMs: 60000,
      diskEnabled: true,
      diskPath: join(tempDir, 'cache2.db'),
      diskTtlMs: 60000,
    });
    await cache3.initialize();

    const result = await cache3.get('persistent');
    expect(result).toBe('data');

    await cache3.dispose();
  });

  test('should delete from both tiers', async () => {
    await cache.set('key1', 'value1');

    // Verify in memory
    expect(await cache.get('key1')).toBe('value1');

    await cache.delete('key1');

    expect(await cache.get('key1')).toBeUndefined();
  });

  test('should clear both tiers', async () => {
    await cache.set('a', '1');
    await cache.set('b', '2');

    await cache.clear();

    expect(await cache.get('a')).toBeUndefined();
    expect(await cache.get('b')).toBeUndefined();
  });

  test('should report combined stats', async () => {
    await cache.set('key1', 'value1');
    await cache.get('key1'); // Hit
    await cache.get('key1'); // Hit
    await cache.get('nonexistent'); // Miss

    const stats = cache.getStats();
    expect(stats.memory.hits).toBe(2);
    expect(stats.memory.misses).toBe(1);
    expect(stats.hitRate).toBe(0.5);
  });

  test('should perform health check', async () => {
    const health = await cache.healthCheck();
    expect(health.healthy).toBe(true);
    expect(health.checks.has('memory')).toBe(true);
    expect(health.checks.has('disk')).toBe(true);
  });

  test('should dispose correctly', async () => {
    await cache.set('key1', 'value1');

    // Dispose should close disk database
    await cache.dispose();

    // After dispose, memory is cleared and disk DB is closed
    // So we expect undefined
    const result = await cache.get('key1');
    expect(result).toBeUndefined();
  });

  test('should work with disk disabled', async () => {
    const memoryOnly = new CacheService({
      memoryMaxSize: 100,
      memoryTtlMs: 5000,
      diskEnabled: false,
    });
    await memoryOnly.initialize();

    await memoryOnly.set('key', 'value');
    expect(await memoryOnly.get('key')).toBe('value');

    const stats = memoryOnly.getStats();
    expect(stats.disk).toBeUndefined();

    await memoryOnly.dispose();
  });
});
