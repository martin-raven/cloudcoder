use std::sync::Arc;

use serde::{de::DeserializeOwned, Serialize};

use cloudcoder_core::{CacheOptions, CacheStats, Service, ServiceError, HealthStatus, HealthCheck, CloudCoderError};

use super::memory::MemoryCache;
use super::disk::DiskCache;

/// Combined memory and disk cache service
pub struct CacheService {
    memory: Arc<tokio::sync::RwLock<MemoryCache<Vec<u8>>>>,
    disk: Option<Arc<DiskCache>>,
}

impl CacheService {
    pub fn new(memory_options: CacheOptions) -> Self {
        Self {
            memory: Arc::new(tokio::sync::RwLock::new(MemoryCache::new(memory_options))),
            disk: None,
        }
    }

    pub fn with_disk(memory_options: CacheOptions, disk_path: impl Into<String>) -> Self {
        Self {
            memory: Arc::new(tokio::sync::RwLock::new(MemoryCache::new(memory_options))),
            disk: Some(Arc::new(DiskCache::new(disk_path))),
        }
    }

    pub fn with_memory_disk(memory_options: CacheOptions) -> Self {
        Self {
            memory: Arc::new(tokio::sync::RwLock::new(MemoryCache::new(memory_options))),
            disk: Some(Arc::new(DiskCache::memory())),
        }
    }

    /// Get a value from cache (memory first, then disk)
    pub async fn get<T: DeserializeOwned + Serialize + Clone + Send + Sync + 'static>(&self, key: &str) -> Result<Option<T>, CloudCoderError> {
        // Try memory first
        {
            let mut mem = self.memory.write().await;
            if let Some(bytes) = mem.get(key) {
                let value: T = bincode::deserialize(&bytes)
                    .map_err(|e| CloudCoderError::Cache(format!("Deserialization failed: {}", e)))?;
                return Ok(Some(value));
            }
        }

        // Try disk if available
        if let Some(ref disk) = self.disk {
            if let Some(value) = disk.get::<T>(key)
                .map_err(|e| CloudCoderError::Cache(e.message))? {
                // Populate memory cache
                let bytes = bincode::serialize(&value)
                    .map_err(|e| CloudCoderError::Cache(format!("Serialization failed: {}", e)))?;
                let mut mem = self.memory.write().await;
                mem.set(key.to_string(), bytes);
                return Ok(Some(value));
            }
        }

        Ok(None)
    }

    /// Set a value in cache (memory and disk)
    pub async fn set<T: Serialize + Clone + Send + Sync + 'static>(&self, key: String, value: &T) -> Result<(), CloudCoderError> {
        let bytes = bincode::serialize(value)
            .map_err(|e| CloudCoderError::Cache(format!("Serialization failed: {}", e)))?;

        // Set in memory
        {
            let mut mem = self.memory.write().await;
            mem.set(key.clone(), bytes);
        }

        // Set in disk if available
        if let Some(ref disk) = self.disk {
            disk.set(&key, value)
                .map_err(|e| CloudCoderError::Cache(e.message))?;
        }

        Ok(())
    }

    /// Check if a key exists (memory first, then disk)
    pub async fn has(&self, key: &str) -> bool {
        // Check memory first
        {
            let mem = self.memory.read().await;
            if mem.has(key) {
                return true;
            }
        }

        // Check disk if available
        if let Some(ref disk) = self.disk {
            return disk.has(key).unwrap_or(false);
        }

        false
    }

    /// Delete a key from cache (memory and disk)
    pub async fn delete(&self, key: &str) -> Result<(), CloudCoderError> {
        // Delete from memory
        {
            let mut mem = self.memory.write().await;
            mem.delete(key);
        }

        // Delete from disk if available
        if let Some(ref disk) = self.disk {
            disk.delete(key)
                .map_err(|e| CloudCoderError::Cache(e.message))?;
        }

        Ok(())
    }

    /// Clear all cache entries (memory and disk)
    pub async fn clear(&self) -> Result<(), CloudCoderError> {
        // Clear memory
        {
            let mut mem = self.memory.write().await;
            mem.clear();
        }

        // Clear disk if available
        if let Some(ref disk) = self.disk {
            disk.clear()
                .map_err(|e| CloudCoderError::Cache(e.message))?;
        }

        Ok(())
    }

    /// Get combined cache statistics
    pub async fn get_stats(&self) -> CacheStats {
        let mem = self.memory.read().await;
        mem.get_stats()
    }
}

impl Service for CacheService {
    fn name(&self) -> &str {
        "CacheService"
    }

    fn initialize<'a>(&'a mut self) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), ServiceError>> + Send + 'a>> {
        Box::pin(async move {
            // For now, disk cache is initialized lazily on first use
            // If we want explicit initialization, we could call disk.initialize() here
            Ok(())
        })
    }

    fn dispose<'a>(&'a mut self) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), ServiceError>> + Send + 'a>> {
        Box::pin(async move {
            // Clear memory cache
            let mut mem = self.memory.write().await;
            mem.clear();
            Ok(())
        })
    }

    fn health_check(&self) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<HealthStatus, ServiceError>> + Send + '_>> {
        Box::pin(async move {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64;

            let mem_stats = {
                let mem = self.memory.read().await;
                mem.get_stats()
            };

            let mut checks = std::collections::HashMap::new();

            checks.insert(
                "memory_cache".to_string(),
                HealthCheck {
                    ok: true,
                    message: Some(format!(
                        "Size: {}, Hits: {}, Misses: {}",
                        mem_stats.size, mem_stats.hits, mem_stats.misses
                    )),
                    value: Some(mem_stats.size as f64),
                },
            );

            if let Some(ref disk) = self.disk {
                match disk.health_check().await {
                    Ok(disk_status) => {
                        checks.insert("disk_cache".to_string(), disk_status.checks.get("disk_cache").cloned().unwrap_or(HealthCheck {
                            ok: disk_status.healthy,
                            message: None,
                            value: None,
                        }));
                    }
                    Err(e) => {
                        checks.insert(
                            "disk_cache".to_string(),
                            HealthCheck {
                                ok: false,
                                message: Some(e.message),
                                value: None,
                            },
                        );
                    }
                }
            }

            let healthy = checks.values().all(|c| c.ok);

            Ok(HealthStatus {
                healthy,
                checks,
                last_check: now,
            })
        })
    }
}