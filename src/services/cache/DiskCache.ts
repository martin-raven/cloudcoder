/**
 * SQLite-based disk cache for persistent caching.
 * Uses bun:sqlite for optimal performance.
 */

import { Database } from 'bun:sqlite';
import type { Service, HealthStatus } from '../../types/core.js';

interface DiskCacheOptions {
  dbPath: string;
  ttlMs?: number;
}

export class DiskCache implements Service {
  private db: Database | null = null;
  private readonly dbPath: string;
  private readonly ttlMs: number;
  private initialized = false;

  constructor(options: DiskCacheOptions) {
    this.dbPath = options.dbPath;
    this.ttlMs = options.ttlMs ?? 3600000; // 1 hour default
  }

  async initialize(): Promise<void> {
    if (this.initialized) return;

    this.db = new Database(this.dbPath);

    // Create cache table
    this.db.exec(`
      CREATE TABLE IF NOT EXISTS cache (
        key TEXT PRIMARY KEY,
        value TEXT NOT NULL,
        expiresAt INTEGER NOT NULL
      )
    `);

    // Create index for expiration queries
    this.db.exec(`
      CREATE INDEX IF NOT EXISTS idx_expires ON cache(expiresAt)
    `);

    this.initialized = true;
  }

  async set(key: string, value: unknown, ttlMs?: number): Promise<void> {
    if (!this.initialized) await this.initialize();

    const expiresAt = Date.now() + (ttlMs ?? this.ttlMs);
    const valueJson = JSON.stringify(value);

    this.db!.exec(
      'INSERT OR REPLACE INTO cache (key, value, expiresAt) VALUES (?, ?, ?)',
      key,
      valueJson,
      expiresAt
    );
  }

  async get<T>(key: string): Promise<T | undefined> {
    if (!this.initialized) await this.initialize();

    const row = this.db!.query('SELECT value, expiresAt FROM cache WHERE key = ?').get(key) as
      | { value: string; expiresAt: number }
      | undefined;

    if (!row) {
      return undefined;
    }

    // Check expiration
    if (Date.now() > row.expiresAt) {
      await this.delete(key);
      return undefined;
    }

    return JSON.parse(row.value) as T;
  }

  async has(key: string): Promise<boolean> {
    const value = await this.get(key);
    return value !== undefined;
  }

  async delete(key: string): Promise<void> {
    if (!this.initialized) await this.initialize();

    this.db!.exec('DELETE FROM cache WHERE key = ?', key);
  }

  async clear(): Promise<void> {
    if (!this.initialized) await this.initialize();

    this.db!.exec('DELETE FROM cache');
  }

  async dispose(): Promise<void> {
    if (this.db) {
      this.db.close();
      this.db = null;
      this.initialized = false;
    }
  }

  async healthCheck(): Promise<HealthStatus> {
    const checks = new Map();

    try {
      if (!this.initialized) {
        await this.initialize();
      }

      // Test read/write
      const testKey = `_health_${Date.now()}`;
      await this.set(testKey, 'health');
      const result = await this.get(testKey);
      await this.delete(testKey);

      checks.set('database', { ok: result === 'health' });
      checks.set('initialized', { ok: this.initialized });
    } catch (err) {
      checks.set('database', { ok: false, message: (err as Error).message });
    }

    return {
      healthy: Array.from(checks.values()).every(c => c.ok),
      checks,
      lastCheck: Date.now(),
    };
  }

  get name(): string {
    return 'DiskCache';
  }
}
