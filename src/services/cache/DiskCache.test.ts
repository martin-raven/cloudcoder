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

  test('should check if key exists', async () => {
    await cache.set('exists', 'value');
    expect(await cache.has('exists')).toBe(true);
    expect(await cache.has('notexists')).toBe(false);
  });

  test('should perform health check', async () => {
    const health = await cache.healthCheck();
    expect(health.healthy).toBe(true);
    expect(health.checks.get('database')?.ok).toBe(true);
  });
});
