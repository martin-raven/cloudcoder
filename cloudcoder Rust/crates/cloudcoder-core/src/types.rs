use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;

use crate::error::ServiceError;

/// Permission behavior for tool execution
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ToolPermissionBehavior {
    Allow,
    Deny,
    Ask,
}

/// Result of a permission check for tool execution
#[derive(Debug, Clone)]
pub struct ToolPermissionResult {
    pub behavior: ToolPermissionBehavior,
    pub updated_input: Option<String>, // JSON string representation
    pub reason: Option<String>,
}

/// Options for cache configuration
#[derive(Debug, Clone)]
pub struct CacheOptions {
    pub max_size: usize,
    pub ttl_ms: u64,
}

impl Default for CacheOptions {
    fn default() -> Self {
        Self {
            max_size: 1000,
            ttl_ms: 300000,
        }
    }
}

/// Statistics for cache operations
#[derive(Debug, Clone, Default)]
pub struct CacheStats {
    pub size: usize,
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
}

/// Health check result for a single component
#[derive(Debug, Clone)]
pub struct HealthCheck {
    pub ok: bool,
    pub message: Option<String>,
    pub value: Option<f64>,
}

/// Overall health status of a service
#[derive(Debug, Clone)]
pub struct HealthStatus {
    pub healthy: bool,
    pub checks: HashMap<String, HealthCheck>,
    pub last_check: u64, // timestamp in ms
}

/// Service trait for lifecycle management
pub trait Service: Send + Sync {
    fn name(&self) -> &str;

    fn initialize<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = Result<(), ServiceError>> + Send + 'a>>;

    fn dispose<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = Result<(), ServiceError>> + Send + 'a>>;

    fn health_check(&self) -> Pin<Box<dyn Future<Output = Result<HealthStatus, ServiceError>> + Send + '_>>;
}

/// Type alias for lazy loader function
pub type LazyLoader<T> = Box<
    dyn Fn() -> Pin<Box<dyn Future<Output = Result<T, crate::error::CloudCoderError>> + Send>> + Send + Sync,
>;