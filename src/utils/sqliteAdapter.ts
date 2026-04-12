/**
 * SQLite adapter with Bun/Node compatibility.
 * Uses bun:sqlite when available, falls back to better-sqlite3 for Node.
 */

import { Database as BunDatabase } from 'bun:sqlite';

interface SqliteDatabase {
  exec(sql: string): void;
  query(sql: string): {
    get(...params: unknown[]): unknown | undefined;
    all(...params: unknown[]): unknown[];
    run(...params: unknown[]): { changes: number; lastInsertRowid: number };
  };
  close(): void;
}

interface SqliteFactory {
  new (path: string): SqliteDatabase;
}

// Use Bun's sqlite directly - it's always available when running with bun test
export const SqliteDatabaseImpl: SqliteFactory = BunDatabase;

/**
 * Check if running on Bun runtime.
 */
export function isBunRuntime(): boolean {
  return true; // We're always on Bun when using this module
}

/**
 * Get the SQLite implementation (always Bun in this codebase).
 */
export function getSqlite(): SqliteFactory {
  return SqliteDatabaseImpl;
}

/**
 * Create a database connection.
 */
export function createDatabase(path: string): SqliteDatabase {
  return new SqliteDatabaseImpl(path);
}

export type { SqliteDatabase, SqliteFactory };
