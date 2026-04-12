import { describe, test, expect, beforeEach } from 'bun:test';
import { FileWatcherCache } from './FileWatcherCache.js';
import { MemoryCache } from './MemoryCache.js';
import { EventBus } from '../../utils/events/EventBus.js';

describe('FileWatcherCache', () => {
  let cache: MemoryCache;
  let fileWatcher: FileWatcherCache;
  let eventBus: EventBus;

  beforeEach(() => {
    cache = new MemoryCache({ maxSize: 100, ttlMs: 60000 });
    eventBus = new EventBus();
    fileWatcher = new FileWatcherCache({
      cache,
      eventBus,
      debounceMs: 10,
    });
  });

  test('should watch files and track them', () => {
    fileWatcher.watch('/path/to/file.ts', 'cache-key-1');

    const stats = fileWatcher.getWatchStats();
    expect(stats.watchedCount).toBe(1);
  });

  test('should invalidate cache on file change', async () => {
    // Set up watched file with cached value
    fileWatcher.watch('/path/to/file.ts', 'file-content');
    await fileWatcher.set('file-content', 'cached-value');

    expect(await fileWatcher.get('file-content')).toBe('cached-value');

    // Trigger file change
    fileWatcher.invalidate('/path/to/file.ts');

    // Wait for debounce
    await new Promise(resolve => setTimeout(resolve, 20));

    expect(await fileWatcher.get('file-content')).toBeUndefined();
  });

  test('should debounce multiple changes', async () => {
    fileWatcher.watch('/path/to/file.ts', 'file-key');
    await fileWatcher.set('file-key', 'value');

    // Trigger multiple changes rapidly
    fileWatcher.invalidate('/path/to/file.ts');
    fileWatcher.invalidate('/path/to/file.ts');
    fileWatcher.invalidate('/path/to/file.ts');

    const stats1 = fileWatcher.getWatchStats();
    expect(stats1.pendingInvalidations).toBe(1); // Should be debounced to 1

    // Wait for debounce to fire and clear timer
    await new Promise(resolve => setTimeout(resolve, 50));

    // After debounce fires, cache should be invalidated
    expect(await fileWatcher.get('file-key')).toBeUndefined();
  });

  test('should unwatch files and clear timers', () => {
    fileWatcher.watch('/path/to/file.ts', 'file-key');
    fileWatcher.invalidate('/path/to/file.ts');

    // Unwatch before debounce fires
    fileWatcher.unwatch('/path/to/file.ts');

    const stats = fileWatcher.getWatchStats();
    expect(stats.watchedCount).toBe(0);
  });

  test('should clear all watched files', () => {
    fileWatcher.watch('/file1.ts', 'key1');
    fileWatcher.watch('/file2.ts', 'key2');
    fileWatcher.watch('/file3.ts', 'key3');

    fileWatcher.clearWatched();

    const stats = fileWatcher.getWatchStats();
    expect(stats.watchedCount).toBe(0);
    expect(stats.pendingInvalidations).toBe(0);
  });

  test('should emit invalidation events', async () => {
    const events: unknown[] = [];

    eventBus.subscribe('settings_change', (event) => {
      events.push(event.payload);
    });

    fileWatcher.watch('/path/to/file.ts', 'file-key');
    fileWatcher.invalidate('/path/to/file.ts');

    await new Promise(resolve => setTimeout(resolve, 20));

    expect(events).toHaveLength(1);
    expect(events[0]).toMatchObject({
      type: 'cache_invalidation',
      path: '/path/to/file.ts',
      cacheKey: 'file-key',
    });
  });

  test('should delegate cache operations', async () => {
    await fileWatcher.set('key1', 'value1');
    expect(await fileWatcher.get('key1')).toBe('value1');
    expect(await fileWatcher.has('key1')).toBe(true);

    await fileWatcher.delete('key1');
    expect(await fileWatcher.get('key1')).toBeUndefined();

    await fileWatcher.set('a', '1');
    await fileWatcher.set('b', '2');
    await fileWatcher.clear();
    expect(await fileWatcher.get('a')).toBeUndefined();
    expect(await fileWatcher.get('b')).toBeUndefined();
  });
});
