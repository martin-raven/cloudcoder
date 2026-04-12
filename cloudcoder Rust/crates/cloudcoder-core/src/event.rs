use std::collections::HashMap;
use std::sync::Arc;

/// Types of events that can occur in the system
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EventType {
    ToolCallStart,
    ToolCallComplete,
    ToolCallError,
    PermissionCheck,
    ApiRequestStart,
    ApiRequestComplete,
    ContextCompactStart,
    ContextCompactComplete,
    SessionStart,
    SessionEnd,
    SettingsChange,
    CacheInvalidation,
}

/// An event with payload and metadata
#[derive(Debug, Clone)]
pub struct CloudCoderEvent<T: Send + Sync> {
    pub event_type: EventType,
    pub payload: T,
    pub timestamp: u64, // ms
    pub source: String,
}

/// Statistics for the event bus
#[derive(Debug, Clone, Default)]
pub struct EventBusStats {
    pub total_events_emitted: u64,
    pub events_by_type: HashMap<EventType, u64>,
}

/// Options for configuring the event bus
#[derive(Debug, Clone)]
pub struct EventBusOptions {
    pub max_buffered_events: usize,
    pub debug: bool,
}

impl Default for EventBusOptions {
    fn default() -> Self {
        Self {
            max_buffered_events: 100,
            debug: false,
        }
    }
}

/// Subscription handle that allows unsubscribing from events
pub struct EventSubscription {
    event_type: Option<EventType>,
    id: usize,
    unsubscribe_fn: Arc<dyn Fn(Option<EventType>, usize) + Send + Sync>,
}

impl EventSubscription {
    pub fn new(
        event_type: Option<EventType>,
        id: usize,
        unsubscribe_fn: Arc<dyn Fn(Option<EventType>, usize) + Send + Sync>,
    ) -> Self {
        Self {
            event_type,
            id,
            unsubscribe_fn,
        }
    }

    /// Unsubscribe from the event bus
    pub fn unsubscribe(self) {
        (self.unsubscribe_fn)(self.event_type, self.id);
    }
}

impl Drop for EventSubscription {
    fn drop(&mut self) {
        // Note: We don't auto-unsubscribe on drop to avoid double-unsubscribe
        // The unsubscribe method should be called explicitly if needed
    }
}

/// Trait for event bus implementations
///
/// This uses `String` as the payload type to avoid external dependencies.
/// Implementations can serialize/deserialize JSON as needed.
pub trait IEventBus: Send + Sync {
    /// Subscribe to a specific event type
    fn subscribe(
        &self,
        event_type: EventType,
        handler: Arc<dyn Fn(&CloudCoderEvent<String>) + Send + Sync>,
    ) -> EventSubscription;

    /// Subscribe to all events
    fn subscribe_all(
        &self,
        handler: Arc<dyn Fn(&CloudCoderEvent<String>) + Send + Sync>,
    ) -> EventSubscription;

    /// Emit an event to all subscribers
    fn emit(&self, event: CloudCoderEvent<String>);

    /// Get event bus statistics
    fn get_stats(&self) -> EventBusStats;

    /// Clear all buffered events
    fn clear(&self);
}