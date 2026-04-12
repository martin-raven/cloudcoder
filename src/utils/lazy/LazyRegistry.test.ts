import { describe, test, expect, beforeEach } from 'bun:test';
import { LazyRegistry } from './LazyRegistry.js';

describe('LazyRegistry', () => {
  let registry: LazyRegistry<string>;

  beforeEach(() => {
    registry = new LazyRegistry();
  });

  test('should lazy load items on first access', async () => {
    let loadCount = 0;

    registry.register('item1', async () => {
      loadCount++;
      return 'loaded-item1';
    });

    expect(loadCount).toBe(0);

    const result = await registry.get('item1');

    expect(result).toBe('loaded-item1');
    expect(loadCount).toBe(1);
  });

  test('should cache loaded items', async () => {
    let loadCount = 0;

    registry.register('item1', async () => {
      loadCount++;
      return 'loaded-item1';
    });

    await registry.get('item1');
    await registry.get('item1');
    await registry.get('item1');

    expect(loadCount).toBe(1);
  });

  test('should return undefined for unregistered items', async () => {
    const result = await registry.get('nonexistent');
    expect(result).toBeUndefined();
  });

  test('should list registered keys without loading', async () => {
    registry.register('a', async () => 'a');
    registry.register('b', async () => 'b');
    registry.register('c', async () => 'c');

    const keys = registry.keys();
    expect(keys).toEqual(['a', 'b', 'c']);

    // Verify nothing was loaded
    expect(registry.isLoaded('a')).toBe(false);
    expect(registry.isLoaded('b')).toBe(false);
    expect(registry.isLoaded('c')).toBe(false);
  });

  test('should support bulk loading', async () => {
    const loadOrder: string[] = [];

    registry.register('a', async () => {
      loadOrder.push('a');
      return 'a';
    });
    registry.register('b', async () => {
      loadOrder.push('b');
      return 'b';
    });

    await registry.loadAll(['b', 'a']);

    expect(loadOrder).toEqual(['b', 'a']);
  });

  test('should share pending promises for concurrent requests', async () => {
    let loadCount = 0;

    registry.register('slow', async () => {
      loadCount++;
      await new Promise(resolve => setTimeout(resolve, 100));
      return 'loaded';
    });

    // Fire 3 concurrent requests
    const [result1, result2, result3] = await Promise.all([
      registry.get('slow'),
      registry.get('slow'),
      registry.get('slow'),
    ]);

    expect(loadCount).toBe(1); // Should only load once
    expect(result1).toBe('loaded');
    expect(result2).toBe('loaded');
    expect(result3).toBe('loaded');
  });

  test('should handle load errors correctly', async () => {
    registry.register('fail', async () => {
      throw new Error('Load failed');
    });

    await expect(registry.get('fail')).rejects.toThrow('Load failed');

    // Should be able to retry
    await expect(registry.get('fail')).rejects.toThrow('Load failed');
  });

  test('should clear all entries', async () => {
    registry.register('a', async () => 'a');
    registry.register('b', async () => 'b');

    await registry.get('a');

    registry.clear();

    expect(registry.keys()).toEqual([]);
    expect(registry.isLoaded('a')).toBe(false);
  });

  test('should track stats correctly', async () => {
    registry.register('success', async () => 'ok');
    registry.register('fail', async () => {
      throw new Error('fail');
    });

    await registry.get('success');
    await registry.get('success'); // Cached
    await registry.get('fail').catch(() => {});

    const stats = registry.getStats();
    expect(stats.totalKeys).toBe(2);
    expect(stats.loadedKeys).toBe(1);
    expect(stats.failedKeys).toBe(1);
  });
});
