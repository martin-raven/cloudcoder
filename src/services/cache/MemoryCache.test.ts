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
    expect(updatedStats.hitRate).toBe(2 / 3);
  });

  test('should handle complex objects', () => {
    const complexValue = {
      nested: { data: [1, 2, 3] },
      timestamp: Date.now(),
    };

    cache.set('complex', complexValue);
    const result = cache.get('complex');
    expect(result).toEqual(complexValue);
  });

  test('should have key', () => {
    cache.set('key1', 'value1');
    expect(cache.has('key1')).toBe(true);
    expect(cache.has('nonexistent')).toBe(false);
  });

  test('should delete entries', () => {
    cache.set('toDelete', 'value');
    expect(cache.has('toDelete')).toBe(true);

    const deleted = cache.delete('toDelete');
    expect(deleted).toBe(true);
    expect(cache.has('toDelete')).toBe(false);

    // Delete non-existent key
    expect(cache.delete('nonexistent')).toBe(false);
  });
});
