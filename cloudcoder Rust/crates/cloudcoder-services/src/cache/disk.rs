use std::sync::Mutex;
use std::future::Future;
use std::pin::Pin;

use rusqlite::Connection;
use serde::{de::DeserializeOwned, Serialize};

use cloudcoder_core::{Service, ServiceError, HealthStatus, HealthCheck};

/// SQLite-based disk cache
pub struct DiskCache {
    conn: Mutex<Option<Connection>>,
    path: String,
}

impl DiskCache {
    pub fn new(path: impl Into<String>) -> Self {
        Self {
            conn: Mutex::new(None),
            path: path.into(),
        }
    }

    pub fn memory() -> Self {
        Self {
            conn: Mutex::new(None),
            path: ":memory:".to_string(),
        }
    }

    fn ensure_initialized(&self) -> Result<(), ServiceError> {
        let mut conn_guard = self.conn.lock().map_err(|e| {
            ServiceError::new(format!("Failed to acquire lock: {}", e))
        })?;

        if conn_guard.is_none() {
            let conn = Connection::open(&self.path).map_err(|e| {
                ServiceError::with_source("Failed to open database", e)
            })?;

            conn.execute(
                "CREATE TABLE IF NOT EXISTS cache (
                    key TEXT PRIMARY KEY,
                    value TEXT NOT NULL,
                    expires_at INTEGER NOT NULL
                )",
                [],
            ).map_err(|e| ServiceError::with_source("Failed to create table", e))?;

            conn.execute(
                "CREATE INDEX IF NOT EXISTS idx_expires_at ON cache(expires_at)",
                [],
            ).map_err(|e| ServiceError::with_source("Failed to create index", e))?;

            *conn_guard = Some(conn);
        }

        Ok(())
    }

    fn current_time_ms() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
    }

    /// Set a value with default TTL
    pub fn set<T: Serialize>(&self, key: &str, value: &T) -> Result<(), ServiceError> {
        self.set_with_ttl(key, value, 300000) // 5 minutes default
    }

    /// Set a value with custom TTL in milliseconds
    pub fn set_with_ttl<T: Serialize>(&self, key: &str, value: &T, ttl_ms: u64) -> Result<(), ServiceError> {
        self.ensure_initialized()?;

        let now = Self::current_time_ms();
        let expires_at = now.saturating_add(ttl_ms);
        let json = serde_json::to_string(value).map_err(|e| {
            ServiceError::with_source("Failed to serialize value", e)
        })?;

        let conn_guard = self.conn.lock().map_err(|e| {
            ServiceError::new(format!("Failed to acquire lock: {}", e))
        })?;

        if let Some(conn) = conn_guard.as_ref() {
            conn.execute(
                "INSERT OR REPLACE INTO cache (key, value, expires_at) VALUES (?1, ?2, ?3)",
                rusqlite::params![key, json, expires_at as i64],
            ).map_err(|e| ServiceError::with_source("Failed to insert cache entry", e))?;
        }

        Ok(())
    }

    /// Get a value if it exists and hasn't expired
    pub fn get<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>, ServiceError> {
        self.ensure_initialized()?;

        let now = Self::current_time_ms();

        let conn_guard = self.conn.lock().map_err(|e| {
            ServiceError::new(format!("Failed to acquire lock: {}", e))
        })?;

        if let Some(conn) = conn_guard.as_ref() {
            let result: Result<(String, i64), _> = conn.query_row(
                "SELECT value, expires_at FROM cache WHERE key = ?1 AND expires_at > ?2",
                rusqlite::params![key, now as i64],
                |row| Ok((row.get(0)?, row.get(1)?)),
            );

            match result {
                Ok((json, _)) => {
                    let value: T = serde_json::from_str(&json).map_err(|e| {
                        ServiceError::with_source("Failed to deserialize value", e)
                    })?;
                    Ok(Some(value))
                }
                Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                Err(e) => Err(ServiceError::with_source("Failed to query cache", e)),
            }
        } else {
            Ok(None)
        }
    }

    /// Check if a key exists and hasn't expired
    pub fn has(&self, key: &str) -> Result<bool, ServiceError> {
        self.ensure_initialized()?;

        let now = Self::current_time_ms();

        let conn_guard = self.conn.lock().map_err(|e| {
            ServiceError::new(format!("Failed to acquire lock: {}", e))
        })?;

        if let Some(conn) = conn_guard.as_ref() {
            let exists: bool = conn.query_row(
                "SELECT 1 FROM cache WHERE key = ?1 AND expires_at > ?2",
                rusqlite::params![key, now as i64],
                |_| Ok(true),
            ).unwrap_or(false);

            Ok(exists)
        } else {
            Ok(false)
        }
    }

    /// Delete a key from the cache
    pub fn delete(&self, key: &str) -> Result<bool, ServiceError> {
        self.ensure_initialized()?;

        let conn_guard = self.conn.lock().map_err(|e| {
            ServiceError::new(format!("Failed to acquire lock: {}", e))
        })?;

        if let Some(conn) = conn_guard.as_ref() {
            let rows = conn.execute(
                "DELETE FROM cache WHERE key = ?1",
                [key],
            ).map_err(|e| ServiceError::with_source("Failed to delete cache entry", e))?;

            Ok(rows > 0)
        } else {
            Ok(false)
        }
    }

    /// Clear all cache entries
    pub fn clear(&self) -> Result<(), ServiceError> {
        self.ensure_initialized()?;

        let conn_guard = self.conn.lock().map_err(|e| {
            ServiceError::new(format!("Failed to acquire lock: {}", e))
        })?;

        if let Some(conn) = conn_guard.as_ref() {
            conn.execute("DELETE FROM cache", [])
                .map_err(|e| ServiceError::with_source("Failed to clear cache", e))?;
        }

        Ok(())
    }

    /// Clean up expired entries
    pub fn cleanup_expired(&self) -> Result<usize, ServiceError> {
        self.ensure_initialized()?;

        let now = Self::current_time_ms();

        let conn_guard = self.conn.lock().map_err(|e| {
            ServiceError::new(format!("Failed to acquire lock: {}", e))
        })?;

        if let Some(conn) = conn_guard.as_ref() {
            let rows = conn.execute(
                "DELETE FROM cache WHERE expires_at <= ?1",
                [now as i64],
            ).map_err(|e| ServiceError::with_source("Failed to cleanup expired entries", e))?;

            Ok(rows)
        } else {
            Ok(0)
        }
    }

    /// Get the number of entries in the cache
    pub fn count(&self) -> Result<usize, ServiceError> {
        self.ensure_initialized()?;

        let conn_guard = self.conn.lock().map_err(|e| {
            ServiceError::new(format!("Failed to acquire lock: {}", e))
        })?;

        if let Some(conn) = conn_guard.as_ref() {
            let count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM cache",
                [],
                |row| row.get(0),
            ).map_err(|e| ServiceError::with_source("Failed to count cache entries", e))?;

            Ok(count as usize)
        } else {
            Ok(0)
        }
    }
}

impl Service for DiskCache {
    fn name(&self) -> &str {
        "DiskCache"
    }

    fn initialize<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = Result<(), ServiceError>> + Send + 'a>> {
        Box::pin(async move {
            self.ensure_initialized()
        })
    }

    fn dispose<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = Result<(), ServiceError>> + Send + 'a>> {
        Box::pin(async move {
            let mut conn_guard = self.conn.lock().map_err(|e| {
                ServiceError::new(format!("Failed to acquire lock: {}", e))
            })?;
            *conn_guard = None;
            Ok(())
        })
    }

    fn health_check(&self) -> Pin<Box<dyn Future<Output = Result<HealthStatus, ServiceError>> + Send + '_>> {
        Box::pin(async move {
            let now = Self::current_time_ms();

            // Try a simple write and read
            let test_key = "__health_check__";
            let test_value = now;

            self.set_with_ttl(test_key, &test_value, 5000)?;

            let read_value: Option<u64> = self.get(test_key)?;
            let ok = read_value == Some(test_value);

            // Clean up test entry
            let _ = self.delete(test_key);

            Ok(HealthStatus {
                healthy: ok,
                checks: [(
                    "disk_cache".to_string(),
                    HealthCheck {
                        ok,
                        message: if ok { Some("Disk cache is operational".to_string()) } else { Some("Disk cache read/write failed".to_string()) },
                        value: None,
                    },
                )].into_iter().collect(),
                last_check: now,
            })
        })
    }
}