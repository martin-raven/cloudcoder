use std::collections::HashMap;
use std::sync::Arc;
use std::future::Future;
use std::pin::Pin;

use cloudcoder_core::CloudCoderError;
use tokio::sync::RwLock;

enum LazyState<T> {
    NotLoaded,
    Loading(Arc<tokio::sync::Notify>),
    Loaded(T),
    Error(CloudCoderError),
}

struct LazyEntry<T: Clone + Send + Sync + 'static> {
    loader: Option<Arc<dyn Fn() -> Pin<Box<dyn Future<Output = Result<T, CloudCoderError>> + Send>> + Send + Sync>>,
    state: Arc<RwLock<LazyState<T>>>,
}

/// A registry for lazy-loading values on demand
pub struct LazyRegistry<T: Clone + Send + Sync + 'static> {
    entries: RwLock<HashMap<String, LazyEntry<T>>>,
}

impl<T: Clone + Send + Sync + 'static> LazyRegistry<T> {
    pub fn new() -> Self {
        Self {
            entries: RwLock::new(HashMap::new()),
        }
    }

    /// Register a loader for a key
    pub async fn register<F>(&self, key: String, loader: F)
    where
        F: Fn() -> Pin<Box<dyn Future<Output = Result<T, CloudCoderError>> + Send>> + Send + Sync + 'static,
    {
        let mut entries = self.entries.write().await;
        entries.insert(key, LazyEntry {
            loader: Some(Arc::new(loader)),
            state: Arc::new(RwLock::new(LazyState::NotLoaded)),
        });
    }

    /// Get a value, loading it lazily if needed
    pub async fn get(&self, key: &str) -> Result<T, CloudCoderError> {
        // First check if we need to start loading
        let notify = {
            let entries = self.entries.read().await;
            let entry = entries.get(key).ok_or_else(|| {
                CloudCoderError::Cache(format!("Key '{}' not found in registry", key))
            })?;

            let state = entry.state.read().await;
            match &*state {
                LazyState::Loaded(value) => return Ok(value.clone()),
                LazyState::Error(e) => return Err(e.clone()),
                LazyState::NotLoaded => {
                    // Need to start loading - drop state and entries locks first
                    None
                }
                LazyState::Loading(notify) => {
                    // Someone else is loading, return the notify to wait on
                    Some(Arc::clone(notify))
                }
            }
        };

        // If someone else is loading, wait for them
        if let Some(notify) = notify {
            notify.notified().await;
            // Now check the state again
            let entries = self.entries.read().await;
            let entry = entries.get(key).ok_or_else(|| {
                CloudCoderError::Cache(format!("Key '{}' not found in registry", key))
            })?;
            let state = entry.state.read().await;
            return match &*state {
                LazyState::Loaded(value) => Ok(value.clone()),
                LazyState::Error(e) => Err(e.clone()),
                LazyState::NotLoaded => Err(CloudCoderError::Cache("Unexpected state after wait".to_string())),
                LazyState::Loading(_) => Err(CloudCoderError::Cache("Still loading after wait".to_string())),
            };
        }

        // We need to do the loading
        let loader = {
            let entries = self.entries.read().await;
            let entry = entries.get(key).ok_or_else(|| {
                CloudCoderError::Cache(format!("Key '{}' not found in registry", key))
            })?;
            Arc::clone(entry.loader.as_ref().unwrap())
        };

        // Set state to loading
        let notify = Arc::new(tokio::sync::Notify::new());
        let state_arc = {
            let entries = self.entries.read().await;
            let entry = entries.get(key).unwrap();
            Arc::clone(&entry.state)
        };

        {
            let mut state = state_arc.write().await;
            *state = LazyState::Loading(Arc::clone(&notify));
        }

        // Perform the load
        let result = loader().await;

        // Update state
        {
            let mut state = state_arc.write().await;
            match result {
                Ok(value) => {
                    *state = LazyState::Loaded(value);
                }
                Err(e) => {
                    *state = LazyState::Error(e);
                }
            }
        }

        // Notify all waiters
        notify.notify_waiters();

        // Return the result
        let entries = self.entries.read().await;
        let entry = entries.get(key).unwrap();
        let state = entry.state.read().await;
        match &*state {
            LazyState::Loaded(value) => Ok(value.clone()),
            LazyState::Error(e) => Err(e.clone()),
            _ => Err(CloudCoderError::Cache("Unexpected state after load".to_string())),
        }
    }

    /// List all registered keys without loading
    pub async fn keys(&self) -> Vec<String> {
        let entries = self.entries.read().await;
        entries.keys().cloned().collect()
    }

    /// Check if a key has been loaded
    pub async fn is_loaded(&self, key: &str) -> bool {
        let entries = self.entries.read().await;
        if let Some(entry) = entries.get(key) {
            let state = entry.state.read().await;
            matches!(&*state, LazyState::Loaded(_))
        } else {
            false
        }
    }

    /// Clear all entries
    pub async fn clear(&self) {
        let mut entries = self.entries.write().await;
        entries.clear();
    }
}

impl<T: Clone + Send + Sync + 'static> Default for LazyRegistry<T> {
    fn default() -> Self {
        Self::new()
    }
}