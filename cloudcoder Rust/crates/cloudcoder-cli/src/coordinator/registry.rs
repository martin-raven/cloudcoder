//! Worker registry for CloudCoder coordinator mode.
//!
//! This module provides a thread-safe registry for tracking worker processes.
//! It maintains an O(1) lookup table for active workers and a chronological
//! event history for auditing and debugging.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use thiserror::Error;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use super::worker::{WorkerProcess, WorkerResult, WorkerStatus};

/// Maximum number of events to keep in history before pruning old events.
const MAX_EVENT_HISTORY: usize = 1000;

/// Error type for registry operations
#[derive(Debug, Error)]
pub enum RegistryError {
    /// Worker with the given ID already exists
    #[error("Worker already exists: {0}")]
    AlreadyExists(String),

    /// Worker with the given ID not found
    #[error("Worker not found: {0}")]
    NotFound(String),

    /// Invalid state transition for the worker
    #[error("Invalid state transition for worker {0}: {1}")]
    InvalidTransition(String, String),

    /// Failed to acquire lock on registry
    #[error("Registry lock error")]
    LockError,
}

/// Type of event that occurred for a worker
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WorkerEventType {
    /// Worker was spawned (started)
    Spawned,

    /// Worker completed successfully
    Completed,

    /// Worker failed with an error
    Failed,

    /// Worker was killed (timeout or manual kill)
    Killed,

    /// Worker was continued from a previous session
    Continued,
}

impl std::fmt::Display for WorkerEventType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WorkerEventType::Spawned => write!(f, "spawned"),
            WorkerEventType::Completed => write!(f, "completed"),
            WorkerEventType::Failed => write!(f, "failed"),
            WorkerEventType::Killed => write!(f, "killed"),
            WorkerEventType::Continued => write!(f, "continued"),
        }
    }
}

/// A record of an event that occurred for a worker
#[derive(Debug, Clone)]
pub struct WorkerEvent {
    /// ID of the worker this event is for
    pub worker_id: String,

    /// Type of event that occurred
    pub event_type: WorkerEventType,

    /// Unix timestamp (milliseconds) when the event occurred
    pub timestamp: u64,

    /// Human-readable details about the event
    pub details: String,
}

impl WorkerEvent {
    /// Create a new worker event with the current timestamp
    pub fn new(worker_id: impl Into<String>, event_type: WorkerEventType, details: impl Into<String>) -> Self {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);

        Self {
            worker_id: worker_id.into(),
            event_type,
            timestamp,
            details: details.into(),
        }
    }

    /// Create a new worker event with a specific timestamp
    pub fn with_timestamp(
        worker_id: impl Into<String>,
        event_type: WorkerEventType,
        timestamp: u64,
        details: impl Into<String>,
    ) -> Self {
        Self {
            worker_id: worker_id.into(),
            event_type,
            timestamp,
            details: details.into(),
        }
    }
}

/// Thread-safe registry for tracking worker processes
#[derive(Debug)]
pub struct WorkerRegistry {
    /// Active and recently completed workers by ID
    workers: HashMap<String, WorkerProcess>,

    /// Chronological event history (limited to last MAX_EVENT_HISTORY events)
    history: Vec<WorkerEvent>,
}

impl WorkerRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self {
            workers: HashMap::new(),
            history: Vec::with_capacity(MAX_EVENT_HISTORY),
        }
    }

    /// Register a running worker in the registry
    ///
    /// # Arguments
    ///
    /// * `worker` - The worker process to register
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or a `RegistryError` if the worker ID already exists.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let mut registry = WorkerRegistry::new();
    /// let worker = spawn_worker(config).await?;
    /// registry.register(worker)?;
    /// ```
    pub fn register(&mut self, worker: WorkerProcess) -> Result<(), RegistryError> {
        let worker_id = worker.id().to_string();

        if self.workers.contains_key(&worker_id) {
            warn!("Attempted to register duplicate worker: {}", worker_id);
            return Err(RegistryError::AlreadyExists(worker_id));
        }

        // Record the spawn event
        let event = WorkerEvent::new(
            &worker_id,
            WorkerEventType::Spawned,
            format!("Worker spawned: {}", worker.description()),
        );

        // Add to registry
        self.workers.insert(worker_id.clone(), worker);
        self.add_event(event);

        info!("Registered worker: {}", worker_id);
        debug!("Registry now has {} workers", self.workers.len());

        Ok(())
    }

    /// Get a worker by ID (immutable reference)
    ///
    /// # Arguments
    ///
    /// * `worker_id` - The ID of the worker to find
    ///
    /// # Returns
    ///
    /// An `Option` containing a reference to the worker if found.
    pub fn get(&self, worker_id: &str) -> Option<&WorkerProcess> {
        self.workers.get(worker_id)
    }

    /// Get a worker by ID (mutable reference)
    ///
    /// # Arguments
    ///
    /// * `worker_id` - The ID of the worker to find
    ///
    /// # Returns
    ///
    /// An `Option` containing a mutable reference to the worker if found.
    pub fn get_mut(&mut self, worker_id: &str) -> Option<&mut WorkerProcess> {
        self.workers.get_mut(worker_id)
    }

    /// Mark a worker as completed with the given result
    ///
    /// # Arguments
    ///
    /// * `worker_id` - The ID of the worker to complete
    /// * `result` - The result of the worker's task
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or a `RegistryError`.
    pub fn complete(&mut self, worker_id: &str, result: WorkerResult) -> Result<(), RegistryError> {
        let worker = self.workers.get_mut(worker_id).ok_or_else(|| {
            RegistryError::NotFound(worker_id.to_string())
        })?;

        // Check if this is a valid state transition
        if !worker.status().is_running() {
            return Err(RegistryError::InvalidTransition(
                worker_id.to_string(),
                format!("Cannot complete worker with status {:?}", worker.status()),
            ));
        }

        // Update the worker status
        let summary = result.summary.clone();
        worker.set_status(WorkerStatus::Completed(result));

        // Record the event
        let event = WorkerEvent::new(
            worker_id,
            WorkerEventType::Completed,
            format!("Worker completed: {}", summary),
        );
        self.add_event(event);

        info!("Worker {} marked as completed", worker_id);

        Ok(())
    }

    /// Mark a worker as failed with the given error message
    ///
    /// # Arguments
    ///
    /// * `worker_id` - The ID of the worker to fail
    /// * `error` - The error message
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or a `RegistryError`.
    pub fn fail(&mut self, worker_id: &str, error: String) -> Result<(), RegistryError> {
        let worker = self.workers.get_mut(worker_id).ok_or_else(|| {
            RegistryError::NotFound(worker_id.to_string())
        })?;

        // Check if this is a valid state transition
        if !worker.status().is_running() {
            return Err(RegistryError::InvalidTransition(
                worker_id.to_string(),
                format!("Cannot fail worker with status {:?}", worker.status()),
            ));
        }

        // Update the worker status
        worker.set_status(WorkerStatus::Failed(error.clone()));

        // Record the event
        let event = WorkerEvent::new(
            worker_id,
            WorkerEventType::Failed,
            format!("Worker failed: {}", error),
        );
        self.add_event(event);

        info!("Worker {} marked as failed: {}", worker_id, error);

        Ok(())
    }

    /// Mark a worker as killed
    ///
    /// # Arguments
    ///
    /// * `worker_id` - The ID of the worker to kill
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or a `RegistryError`.
    pub fn kill(&mut self, worker_id: &str) -> Result<(), RegistryError> {
        let worker = self.workers.get_mut(worker_id).ok_or_else(|| {
            RegistryError::NotFound(worker_id.to_string())
        })?;

        // Check if this is a valid state transition
        if !worker.status().is_running() {
            return Err(RegistryError::InvalidTransition(
                worker_id.to_string(),
                format!("Cannot kill worker with status {:?}", worker.status()),
            ));
        }

        // Update the worker status
        worker.set_status(WorkerStatus::Killed);

        // Record the event
        let event = WorkerEvent::new(
            worker_id,
            WorkerEventType::Killed,
            "Worker killed".to_string(),
        );
        self.add_event(event);

        info!("Worker {} marked as killed", worker_id);

        Ok(())
    }

    /// Record that a worker was continued from a previous session
    ///
    /// This is used when a worker is resumed from an existing conversation.
    ///
    /// # Arguments
    ///
    /// * `worker_id` - The ID of the worker
    /// * `conversation_id` - The ID of the conversation being continued
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or a `RegistryError`.
    pub fn record_continuation(&mut self, worker_id: &str, conversation_id: &str) -> Result<(), RegistryError> {
        if !self.workers.contains_key(worker_id) {
            return Err(RegistryError::NotFound(worker_id.to_string()));
        }

        // Record the event
        let event = WorkerEvent::new(
            worker_id,
            WorkerEventType::Continued,
            format!("Worker continued from conversation: {}", conversation_id),
        );
        self.add_event(event);

        debug!("Worker {} continued from conversation {}", worker_id, conversation_id);

        Ok(())
    }

    /// List all currently running workers
    ///
    /// # Returns
    ///
    /// A vector of references to running workers.
    pub fn list_active(&self) -> Vec<&WorkerProcess> {
        self.workers
            .values()
            .filter(|w| w.status().is_running())
            .collect()
    }

    /// List all workers with a specific status
    ///
    /// # Arguments
    ///
    /// * `status_filter` - A function that returns true for workers to include
    ///
    /// # Returns
    ///
    /// A vector of references to workers matching the filter.
    pub fn list_by_status<F>(&self, status_filter: F) -> Vec<&WorkerProcess>
    where
        F: Fn(&WorkerStatus) -> bool,
    {
        self.workers
            .values()
            .filter(|w| status_filter(w.status()))
            .collect()
    }

    /// List all completed workers
    pub fn list_completed(&self) -> Vec<&WorkerProcess> {
        self.list_by_status(|s| matches!(s, WorkerStatus::Completed(_)))
    }

    /// List all failed workers
    pub fn list_failed(&self) -> Vec<&WorkerProcess> {
        self.list_by_status(|s| matches!(s, WorkerStatus::Failed(_)))
    }

    /// List all killed workers
    pub fn list_killed(&self) -> Vec<&WorkerProcess> {
        self.list_by_status(|s| matches!(s, WorkerStatus::Killed))
    }

    /// Get the event history
    ///
    /// # Returns
    ///
    /// A slice of all recorded events (oldest first).
    pub fn get_history(&self) -> &[WorkerEvent] {
        &self.history
    }

    /// Get events for a specific worker
    ///
    /// # Arguments
    ///
    /// * `worker_id` - The ID of the worker
    ///
    /// # Returns
    ///
    /// A vector of events for the specified worker.
    pub fn get_worker_history(&self, worker_id: &str) -> Vec<&WorkerEvent> {
        self.history
            .iter()
            .filter(|e| e.worker_id == worker_id)
            .collect()
    }

    /// Get events of a specific type
    ///
    /// # Arguments
    ///
    /// * `event_type` - The type of events to find
    ///
    /// # Returns
    ///
    /// A vector of events of the specified type.
    pub fn get_events_by_type(&self, event_type: WorkerEventType) -> Vec<&WorkerEvent> {
        self.history
            .iter()
            .filter(|e| e.event_type == event_type)
            .collect()
    }

    /// Clean up completed workers from the registry
    ///
    /// This removes workers that are no longer running to free memory.
    ///
    /// # Returns
    ///
    /// The number of workers removed.
    pub fn cleanup_completed(&mut self) -> usize {
        let before = self.workers.len();

        self.workers.retain(|_, w| w.status().is_running());

        let removed = before - self.workers.len();

        if removed > 0 {
            debug!("Cleaned up {} completed workers", removed);
        }

        removed
    }

    /// Count the number of currently running workers
    ///
    /// # Returns
    ///
    /// The count of running workers.
    pub fn count_active(&self) -> usize {
        self.workers
            .values()
            .filter(|w| w.status().is_running())
            .count()
    }

    /// Count the total number of workers (including completed)
    pub fn count_total(&self) -> usize {
        self.workers.len()
    }

    /// Count workers by status
    ///
    /// # Arguments
    ///
    /// * `status_match` - A function that returns true for workers to count
    ///
    /// # Returns
    ///
    /// The count of workers matching the filter.
    pub fn count_by_status<F>(&self, status_match: F) -> usize
    where
        F: Fn(&WorkerStatus) -> bool,
    {
        self.workers
            .values()
            .filter(|w| status_match(w.status()))
            .count()
    }

    /// Check if a worker exists
    ///
    /// # Arguments
    ///
    /// * `worker_id` - The ID of the worker to check
    ///
    /// # Returns
    ///
    /// `true` if the worker exists, `false` otherwise.
    pub fn contains(&self, worker_id: &str) -> bool {
        self.workers.contains_key(worker_id)
    }

    /// Check if a worker is running
    ///
    /// # Arguments
    ///
    /// * `worker_id` - The ID of the worker to check
    ///
    /// # Returns
    ///
    /// `true` if the worker exists and is running, `false` otherwise.
    pub fn is_running(&self, worker_id: &str) -> bool {
        self.workers
            .get(worker_id)
            .map(|w| w.status().is_running())
            .unwrap_or(false)
    }

    /// Add an event to the history, maintaining the size limit
    fn add_event(&mut self, event: WorkerEvent) {
        // If at capacity, remove oldest events
        if self.history.len() >= MAX_EVENT_HISTORY {
            let excess = self.history.len() - MAX_EVENT_HISTORY + 1;
            self.history.drain(0..excess);
        }

        self.history.push(event);
    }

    /// Get an iterator over all workers
    pub fn iter(&self) -> impl Iterator<Item = (&String, &WorkerProcess)> {
        self.workers.iter()
    }

    /// Get a mutable iterator over all workers
    pub fn iter_mut(&mut self) -> impl Iterator<Item = (&String, &mut WorkerProcess)> {
        self.workers.iter_mut()
    }

    /// Clear all workers and history
    ///
    /// Warning: This should only be used when shutting down.
    pub fn clear(&mut self) {
        self.workers.clear();
        self.history.clear();
    }
}

impl Default for WorkerRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Thread-safe wrapper around WorkerRegistry for async access
///
/// This uses tokio's RwLock to allow multiple concurrent readers
/// or one exclusive writer.
#[derive(Debug, Clone)]
pub struct SharedWorkerRegistry {
    inner: Arc<RwLock<WorkerRegistry>>,
}

impl SharedWorkerRegistry {
    /// Create a new shared registry
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(WorkerRegistry::new())),
        }
    }

    /// Register a worker (requires write lock)
    pub async fn register(&self, worker: WorkerProcess) -> Result<(), RegistryError> {
        let mut registry = self.inner.write().await;
        registry.register(worker)
    }

    /// Get a worker by ID (requires read lock)
    ///
    /// Returns a cloned copy since we can't return references through the lock.
    pub async fn get_cloned(&self, worker_id: &str) -> Option<WorkerSnapshot> {
        let registry = self.inner.read().await;
        registry.get(worker_id).map(|w| WorkerSnapshot {
            id: w.id().to_string(),
            description: w.description().to_string(),
            status: w.status().clone(),
            started_at: w.started_at(),
            runtime_ms: w.get_runtime_ms(),
        })
    }

    /// Check if a worker exists (requires read lock)
    pub async fn contains(&self, worker_id: &str) -> bool {
        let registry = self.inner.read().await;
        registry.contains(worker_id)
    }

    /// Check if a worker is running (requires read lock)
    pub async fn is_running(&self, worker_id: &str) -> bool {
        let registry = self.inner.read().await;
        registry.is_running(worker_id)
    }

    /// Mark a worker as completed (requires write lock)
    pub async fn complete(&self, worker_id: &str, result: WorkerResult) -> Result<(), RegistryError> {
        let mut registry = self.inner.write().await;
        registry.complete(worker_id, result)
    }

    /// Mark a worker as failed (requires write lock)
    pub async fn fail(&self, worker_id: &str, error: String) -> Result<(), RegistryError> {
        let mut registry = self.inner.write().await;
        registry.fail(worker_id, error)
    }

    /// Mark a worker as killed (requires write lock)
    pub async fn kill(&self, worker_id: &str) -> Result<(), RegistryError> {
        let mut registry = self.inner.write().await;
        registry.kill(worker_id)
    }

    /// List all active workers (requires read lock)
    pub async fn list_active(&self) -> Vec<WorkerSnapshot> {
        let registry = self.inner.read().await;
        registry
            .list_active()
            .iter()
            .map(|w| WorkerSnapshot {
                id: w.id().to_string(),
                description: w.description().to_string(),
                status: w.status().clone(),
                started_at: w.started_at(),
                runtime_ms: w.get_runtime_ms(),
            })
            .collect()
    }

    /// Count active workers (requires read lock)
    pub async fn count_active(&self) -> usize {
        let registry = self.inner.read().await;
        registry.count_active()
    }

    /// Cleanup completed workers (requires write lock)
    pub async fn cleanup_completed(&self) -> usize {
        let mut registry = self.inner.write().await;
        registry.cleanup_completed()
    }

    /// Get event history (requires read lock)
    pub async fn get_history(&self) -> Vec<WorkerEvent> {
        let registry = self.inner.read().await;
        registry.get_history().to_vec()
    }

    /// Get the inner registry for exclusive access
    ///
    /// Warning: This acquires a write lock. Use sparingly.
    pub async fn write(&self) -> tokio::sync::RwLockWriteGuard<'_, WorkerRegistry> {
        self.inner.write().await
    }

    /// Get the inner registry for shared access
    pub async fn read(&self) -> tokio::sync::RwLockReadGuard<'_, WorkerRegistry> {
        self.inner.read().await
    }
}

impl Default for SharedWorkerRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// A snapshot of a worker's state
///
/// Used for returning worker info through the async boundary.
#[derive(Debug, Clone)]
pub struct WorkerSnapshot {
    /// Worker ID
    pub id: String,

    /// Worker description
    pub description: String,

    /// Current status
    pub status: WorkerStatus,

    /// When the worker started (Unix timestamp in ms)
    pub started_at: u64,

    /// How long the worker has been running (ms)
    pub runtime_ms: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coordinator::worker::WorkerUsage;

    /// Helper to create a test worker process
    fn create_test_worker(id: &str, description: &str) -> WorkerProcess {
        WorkerProcess::test_new(id, description)
    }

    #[test]
    fn test_registry_creation() {
        let registry = WorkerRegistry::new();
        assert_eq!(registry.count_total(), 0);
        assert_eq!(registry.count_active(), 0);
        assert!(registry.get_history().is_empty());
    }

    #[test]
    fn test_worker_registration() {
        let mut registry = WorkerRegistry::new();
        let worker = create_test_worker("test-1", "Test worker");

        // Register worker
        let result = registry.register(worker);
        assert!(result.is_ok());
        assert_eq!(registry.count_total(), 1);
        assert_eq!(registry.count_active(), 1);

        // Check history was recorded
        let history = registry.get_history();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].event_type, WorkerEventType::Spawned);
        assert_eq!(history[0].worker_id, "test-1");
    }

    #[test]
    fn test_duplicate_registration() {
        let mut registry = WorkerRegistry::new();

        let worker1 = create_test_worker("test-1", "First");
        let worker2 = create_test_worker("test-1", "Second");

        // First registration should succeed
        assert!(registry.register(worker1).is_ok());

        // Second registration with same ID should fail
        let result = registry.register(worker2);
        assert!(matches!(result, Err(RegistryError::AlreadyExists(_))));
        assert_eq!(registry.count_total(), 1);
    }

    #[test]
    fn test_worker_lookup() {
        let mut registry = WorkerRegistry::new();
        let worker = create_test_worker("find-me", "Lookup test");

        registry.register(worker).unwrap();

        // Should find the worker
        let found = registry.get("find-me");
        assert!(found.is_some());
        assert_eq!(found.unwrap().id(), "find-me");

        // Should not find nonexistent worker
        let not_found = registry.get("nonexistent");
        assert!(not_found.is_none());

        // Mutable lookup
        let found_mut = registry.get_mut("find-me");
        assert!(found_mut.is_some());
    }

    #[test]
    fn test_complete_worker() {
        let mut registry = WorkerRegistry::new();
        let worker = create_test_worker("complete-me", "Completion test");

        registry.register(worker).unwrap();

        // Complete the worker
        let result = WorkerResult::new("Task done")
            .with_result("Detailed result")
            .with_usage(WorkerUsage::new(1000, 10, 5000));

        let complete_result = registry.complete("complete-me", result);
        assert!(complete_result.is_ok());

        // Check status changed
        let worker = registry.get("complete-me").unwrap();
        assert!(matches!(worker.status(), WorkerStatus::Completed(_)));

        // Check history
        let history = registry.get_history();
        assert_eq!(history.len(), 2);
        assert_eq!(history[1].event_type, WorkerEventType::Completed);

        // Active count should be 0
        assert_eq!(registry.count_active(), 0);
    }

    #[test]
    fn test_fail_worker() {
        let mut registry = WorkerRegistry::new();
        let worker = create_test_worker("fail-me", "Failure test");

        registry.register(worker).unwrap();

        // Fail the worker
        let result = registry.fail("fail-me", "Something went wrong".to_string());
        assert!(result.is_ok());

        // Check status changed
        let worker = registry.get("fail-me").unwrap();
        assert!(matches!(worker.status(), WorkerStatus::Failed(_)));

        // Check history
        let history = registry.get_history();
        assert_eq!(history.len(), 2);
        assert_eq!(history[1].event_type, WorkerEventType::Failed);

        // Active count should be 0
        assert_eq!(registry.count_active(), 0);
    }

    #[test]
    fn test_kill_worker() {
        let mut registry = WorkerRegistry::new();
        let worker = create_test_worker("kill-me", "Kill test");

        registry.register(worker).unwrap();

        // Kill the worker
        let result = registry.kill("kill-me");
        assert!(result.is_ok());

        // Check status changed
        let worker = registry.get("kill-me").unwrap();
        assert!(matches!(worker.status(), WorkerStatus::Killed));

        // Check history
        let history = registry.get_history();
        assert_eq!(history.len(), 2);
        assert_eq!(history[1].event_type, WorkerEventType::Killed);

        // Active count should be 0
        assert_eq!(registry.count_active(), 0);
    }

    #[test]
    fn test_invalid_transitions() {
        let mut registry = WorkerRegistry::new();
        let worker = create_test_worker("transition-test", "Transition test");

        registry.register(worker).unwrap();

        // Complete the worker
        registry.complete("transition-test", WorkerResult::new("Done")).unwrap();

        // Try to fail a completed worker - should fail
        let result = registry.fail("transition-test", "Too late".to_string());
        assert!(matches!(result, Err(RegistryError::InvalidTransition(_, _))));

        // Try to kill a completed worker - should fail
        let result = registry.kill("transition-test");
        assert!(matches!(result, Err(RegistryError::InvalidTransition(_, _))));
    }

    #[test]
    fn test_not_found_errors() {
        let mut registry = WorkerRegistry::new();

        // Try operations on nonexistent worker
        let result = registry.complete("nonexistent", WorkerResult::new("Done"));
        assert!(matches!(result, Err(RegistryError::NotFound(_))));

        let result = registry.fail("nonexistent", "Error".to_string());
        assert!(matches!(result, Err(RegistryError::NotFound(_))));

        let result = registry.kill("nonexistent");
        assert!(matches!(result, Err(RegistryError::NotFound(_))));
    }

    #[test]
    fn test_list_active() {
        let mut registry = WorkerRegistry::new();

        // Create multiple workers
        for i in 1..=5 {
            let worker = create_test_worker(&format!("worker-{}", i), &format!("Worker {}", i));
            registry.register(worker).unwrap();
        }

        // Complete one
        registry.complete("worker-1", WorkerResult::new("Done")).unwrap();

        // Fail one
        registry.fail("worker-2", "Failed".to_string()).unwrap();

        // Kill one
        registry.kill("worker-3").unwrap();

        // List active - should have 2 workers
        let active = registry.list_active();
        assert_eq!(active.len(), 2);

        // Verify the right workers are active
        let active_ids: Vec<&str> = active.iter().map(|w| w.id()).collect();
        assert!(active_ids.contains(&"worker-4"));
        assert!(active_ids.contains(&"worker-5"));
    }

    #[test]
    fn test_list_by_status() {
        let mut registry = WorkerRegistry::new();

        // Create workers
        for i in 1..=4 {
            let worker = create_test_worker(&format!("status-{}", i), &format!("Worker {}", i));
            registry.register(worker).unwrap();
        }

        // Change statuses
        registry.complete("status-1", WorkerResult::new("Done")).unwrap();
        registry.fail("status-2", "Error".to_string()).unwrap();
        registry.kill("status-3").unwrap();

        // List by status type
        let completed = registry.list_completed();
        assert_eq!(completed.len(), 1);
        assert_eq!(completed[0].id(), "status-1");

        let failed = registry.list_failed();
        assert_eq!(failed.len(), 1);
        assert_eq!(failed[0].id(), "status-2");

        let killed = registry.list_killed();
        assert_eq!(killed.len(), 1);
        assert_eq!(killed[0].id(), "status-3");

        let active = registry.list_active();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].id(), "status-4");
    }

    #[test]
    fn test_cleanup_completed() {
        let mut registry = WorkerRegistry::new();

        // Create workers
        for i in 1..=5 {
            let worker = create_test_worker(&format!("cleanup-{}", i), &format!("Worker {}", i));
            registry.register(worker).unwrap();
        }

        // Complete some
        registry.complete("cleanup-1", WorkerResult::new("Done")).unwrap();
        registry.complete("cleanup-2", WorkerResult::new("Done")).unwrap();
        registry.fail("cleanup-3", "Error".to_string()).unwrap();

        // Cleanup
        let removed = registry.cleanup_completed();
        assert_eq!(removed, 3);

        // Only active should remain
        assert_eq!(registry.count_total(), 2);
        assert_eq!(registry.count_active(), 2);
    }

    #[test]
    fn test_event_history_limit() {
        let mut registry = WorkerRegistry::new();

        // Create more workers than the history limit
        for i in 0..(MAX_EVENT_HISTORY + 100) {
            let worker = create_test_worker(&format!("history-{}", i), &format!("Worker {}", i));
            registry.register(worker).unwrap();
        }

        // History should be limited
        assert_eq!(registry.get_history().len(), MAX_EVENT_HISTORY);

        // Most recent events should be present
        let history = registry.get_history();
        let last_id = format!("history-{}", MAX_EVENT_HISTORY + 99);
        assert!(history.iter().any(|e| e.worker_id == last_id));
    }

    #[test]
    fn test_get_worker_history() {
        let mut registry = WorkerRegistry::new();

        // Create a worker and perform multiple state changes
        let worker = create_test_worker("multi-event", "Multi-event worker");
        registry.register(worker).unwrap();
        registry.complete("multi-event", WorkerResult::new("Done")).unwrap();

        // Create another worker
        let worker2 = create_test_worker("other-worker", "Other worker");
        registry.register(worker2).unwrap();

        // Get history for first worker
        let worker_history = registry.get_worker_history("multi-event");
        assert_eq!(worker_history.len(), 2);
        assert!(worker_history.iter().all(|e| e.worker_id == "multi-event"));

        // History should be in order
        assert_eq!(worker_history[0].event_type, WorkerEventType::Spawned);
        assert_eq!(worker_history[1].event_type, WorkerEventType::Completed);
    }

    #[test]
    fn test_get_events_by_type() {
        let mut registry = WorkerRegistry::new();

        // Create workers with different outcomes
        for i in 1..=3 {
            let worker = create_test_worker(&format!("type-{}", i), &format!("Worker {}", i));
            registry.register(worker).unwrap();
        }

        registry.complete("type-1", WorkerResult::new("Done")).unwrap();
        registry.fail("type-2", "Error".to_string()).unwrap();
        registry.kill("type-3").unwrap();

        // Get events by type
        let completed_events = registry.get_events_by_type(WorkerEventType::Completed);
        assert_eq!(completed_events.len(), 1);

        let failed_events = registry.get_events_by_type(WorkerEventType::Failed);
        assert_eq!(failed_events.len(), 1);

        let killed_events = registry.get_events_by_type(WorkerEventType::Killed);
        assert_eq!(killed_events.len(), 1);

        let spawned_events = registry.get_events_by_type(WorkerEventType::Spawned);
        assert_eq!(spawned_events.len(), 3);
    }

    #[test]
    fn test_record_continuation() {
        let mut registry = WorkerRegistry::new();

        let worker = create_test_worker("continued-worker", "Continued session");
        registry.register(worker).unwrap();

        // Record continuation
        let result = registry.record_continuation("continued-worker", "conv-123");
        assert!(result.is_ok());

        // Check history
        let history = registry.get_worker_history("continued-worker");
        assert_eq!(history.len(), 2);
        assert_eq!(history[1].event_type, WorkerEventType::Continued);
        assert!(history[1].details.contains("conv-123"));
    }

    #[test]
    fn test_contains_and_is_running() {
        let mut registry = WorkerRegistry::new();

        assert!(!registry.contains("check-me"));
        assert!(!registry.is_running("check-me"));

        let worker = create_test_worker("check-me", "Check worker");
        registry.register(worker).unwrap();

        assert!(registry.contains("check-me"));
        assert!(registry.is_running("check-me"));

        registry.complete("check-me", WorkerResult::new("Done")).unwrap();

        assert!(registry.contains("check-me"));
        assert!(!registry.is_running("check-me"));
    }

    #[test]
    fn test_worker_event_creation() {
        let event = WorkerEvent::new("test-worker", WorkerEventType::Spawned, "Worker started");

        assert_eq!(event.worker_id, "test-worker");
        assert_eq!(event.event_type, WorkerEventType::Spawned);
        assert_eq!(event.details, "Worker started");
        assert!(event.timestamp > 0); // Should have a valid timestamp

        // Test with explicit timestamp
        let event_with_ts = WorkerEvent::with_timestamp(
            "test-worker-2",
            WorkerEventType::Completed,
            1234567890,
            "Done",
        );
        assert_eq!(event_with_ts.timestamp, 1234567890);
    }

    #[test]
    fn test_event_type_display() {
        assert_eq!(WorkerEventType::Spawned.to_string(), "spawned");
        assert_eq!(WorkerEventType::Completed.to_string(), "completed");
        assert_eq!(WorkerEventType::Failed.to_string(), "failed");
        assert_eq!(WorkerEventType::Killed.to_string(), "killed");
        assert_eq!(WorkerEventType::Continued.to_string(), "continued");
    }

    #[tokio::test]
    async fn test_shared_registry() {
        let registry = SharedWorkerRegistry::new();

        assert_eq!(registry.count_active().await, 0);

        // Register a worker
        let worker = create_test_worker("shared-1", "Shared test");
        let result = registry.register(worker).await;
        assert!(result.is_ok());

        // Check it exists
        assert!(registry.contains("shared-1").await);
        assert!(registry.is_running("shared-1").await);

        // Get snapshot
        let snapshot = registry.get_cloned("shared-1").await;
        assert!(snapshot.is_some());
        let snapshot = snapshot.unwrap();
        assert_eq!(snapshot.id, "shared-1");

        // Complete it
        registry
            .complete("shared-1", WorkerResult::new("Done"))
            .await
            .unwrap();

        assert_eq!(registry.count_active().await, 0);

        // Cleanup
        let removed = registry.cleanup_completed().await;
        assert_eq!(removed, 1);
    }

    #[test]
    fn test_default_registry() {
        let registry = WorkerRegistry::default();
        assert_eq!(registry.count_total(), 0);
    }

    #[tokio::test]
    async fn test_default_shared_registry() {
        let registry = SharedWorkerRegistry::default();
        assert_eq!(registry.count_active().await, 0);
    }

    #[test]
    fn test_registry_clear() {
        let mut registry = WorkerRegistry::new();

        // Add some workers
        for i in 1..=3 {
            let worker = create_test_worker(&format!("clear-{}", i), &format!("Worker {}", i));
            registry.register(worker).unwrap();
        }

        assert_eq!(registry.count_total(), 3);

        // Clear
        registry.clear();

        assert_eq!(registry.count_total(), 0);
        assert_eq!(registry.get_history().len(), 0);
    }
}