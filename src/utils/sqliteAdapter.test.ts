import { describe, test, expect } from 'bun:test';
import { getSqlite, isBunRuntime, createDatabase } from './sqliteAdapter.js';
import { mkdtemp, rm } from 'fs/promises';
import { tmpdir } from 'os';
import { join } from 'path';

describe('sqliteAdapter', () => {
  test('should detect Bun runtime', () => {
    // We're running on Bun, so this should be true
    expect(isBunRuntime()).toBe(true);
  });

  test('should get SQLite implementation', () => {
    const sqlite = getSqlite();
    expect(sqlite).toBeDefined();
    expect(typeof sqlite).toBe('function');
  });

  test('should create database and perform basic operations', async () => {
    const tempDir = await mkdtemp(join(tmpdir(), 'sqlite-test-'));
    const dbPath = join(tempDir, 'test.db');

    try {
      const db = createDatabase(dbPath);

      // Create table
      db.exec('CREATE TABLE test (id INTEGER PRIMARY KEY, name TEXT)');

      // Insert
      db.exec("INSERT INTO test (name) VALUES ('Alice')");
      db.exec("INSERT INTO test (name) VALUES ('Bob')");

      // Query
      const query = db.query('SELECT * FROM test WHERE name = ?');
      const result = query.get('Alice') as { id: number; name: string };

      expect(result).toBeDefined();
      expect(result.name).toBe('Alice');

      // Get all (use different query without WHERE)
      const allQuery = db.query('SELECT * FROM test');
      const all = allQuery.all() as { id: number; name: string }[];
      expect(all).toHaveLength(2);

      // Close
      db.close();
    } finally {
      await rm(tempDir, { recursive: true, force: true });
    }
  });

  test('should handle in-memory database', () => {
    const db = createDatabase(':memory:');

    db.exec('CREATE TABLE kv (key TEXT PRIMARY KEY, value TEXT)');
    db.exec("INSERT INTO kv VALUES ('key1', 'value1')");

    const query = db.query('SELECT value FROM kv WHERE key = ?');
    const result = query.get('key1') as { value: string };

    expect(result.value).toBe('value1');

    db.close();
  });
});
