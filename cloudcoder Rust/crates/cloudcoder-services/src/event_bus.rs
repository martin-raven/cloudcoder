use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use cloudcoder_core::{EventType, CloudCoderEvent, EventBusOptions, EventBusStats, IEventBus, EventSubscription};
use tokio::sync::RwLock;

type EventHandler = Arc<dyn Fn(&CloudCoderEvent<String>) + Send + Sync>;

struct EventBusInner {
    subscribers: HashMap<EventType, Vec<(usize, EventHandler)>>,
    catch_all_subscribers: Vec<(usize, EventHandler)>,
    event_buffer: Vec<CloudCoderEvent<String>>,
    options: EventBusOptions,
    next_subscription_id: usize,
}

/// Event bus implementation
pub struct EventBus {
    inner: RwLock<EventBusInner>,
    total_events: AtomicU64,
    events_by_type: RwLock<HashMap<EventType, AtomicU64>>,
}

impl EventBus {
    pub fn new(options: EventBusOptions) -> Self {
        Self {
            inner: RwLock::new(EventBusInner {
                subscribers: HashMap::new(),
                catch_all_subscribers: Vec::new(),
                event_buffer: Vec::new(),
                options,
                next_subscription_id: 0,
            }),
            total_events: AtomicU64::new(0),
            events_by_type: RwLock::new(HashMap::new()),
        }
    }
}

impl IEventBus for EventBus {
    fn subscribe(
        &self,
        event_type: EventType,
        handler: EventHandler,
    ) -> EventSubscription {
        let mut inner = self.inner.blocking_write();

        let id = inner.next_subscription_id;
        inner.next_subscription_id += 1;

        inner.subscribers
            .entry(event_type)
            .or_insert_with(Vec::new)
            .push((id, Arc::clone(&handler)));

        drop(inner); // Release the lock

        EventSubscription::new(Some(event_type), id, Arc::new(move |_et, _handler_id| {
            // Unsubscribe functionality is available via EventBus::unsubscribe()
        }))
    }

    fn subscribe_all(
        &self,
        handler: EventHandler,
    ) -> EventSubscription {
        let mut inner = self.inner.blocking_write();

        let id = inner.next_subscription_id;
        inner.next_subscription_id += 1;

        inner.catch_all_subscribers.push((id, Arc::clone(&handler)));

        drop(inner); // Release the lock

        EventSubscription::new(None, id, Arc::new(move |_et, _handler_id| {
            // Unsubscribe functionality is available via EventBus::unsubscribe()
        }))
    }

    fn emit(&self, event: CloudCoderEvent<String>) {
        // Update stats
        self.total_events.fetch_add(1, Ordering::Relaxed);

        let event_type = event.event_type;
        {
            let mut by_type = self.events_by_type.blocking_write();
            by_type
                .entry(event_type)
                .or_insert_with(|| AtomicU64::new(0))
                .fetch_add(1, Ordering::Relaxed);
        }

        // Buffer the event
        let handlers_to_call: Vec<EventHandler>;
        let catch_all_handlers: Vec<EventHandler>;

        {
            let mut inner = self.inner.blocking_write();

            // Add to buffer
            if inner.event_buffer.len() >= inner.options.max_buffered_events {
                inner.event_buffer.remove(0);
            }
            inner.event_buffer.push(event.clone());

            // Collect handlers for this event type
            handlers_to_call = inner.subscribers
                .get(&event_type)
                .map(|v| v.iter().map(|(_, h)| Arc::clone(h)).collect())
                .unwrap_or_default();

            // Collect catch-all handlers
            catch_all_handlers = inner.catch_all_subscribers
                .iter()
                .map(|(_, h)| Arc::clone(h))
                .collect();
        }

        // Call all handlers (error isolation)
        for handler in handlers_to_call.iter().chain(catch_all_handlers.iter()) {
            // Catch panics and log errors
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                handler(&event);
            }));

            if result.is_err() {
                tracing::warn!("Event handler panicked during emit for {:?}", event_type);
            }
        }
    }

    fn get_stats(&self) -> EventBusStats {
        let mut events_by_type = HashMap::new();
        let by_type = self.events_by_type.blocking_read();
        for (k, v) in by_type.iter() {
            events_by_type.insert(*k, v.load(Ordering::Relaxed));
        }

        EventBusStats {
            total_events_emitted: self.total_events.load(Ordering::Relaxed),
            events_by_type,
        }
    }

    fn clear(&self) {
        let mut inner = self.inner.blocking_write();
        inner.event_buffer.clear();
    }
}

impl EventBus {
    /// Unsubscribe a handler by event type and ID
    pub fn unsubscribe(&self, event_type: Option<EventType>, handler_id: usize) {
        let mut inner = self.inner.blocking_write();

        if let Some(et) = event_type {
            if let Some(handlers) = inner.subscribers.get_mut(&et) {
                handlers.retain(|(id, _)| *id != handler_id);
            }
        } else {
            inner.catch_all_subscribers.retain(|(id, _)| *id != handler_id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_bus_creation() {
        let bus = EventBus::new(EventBusOptions::default());
        let stats = bus.get_stats();
        assert_eq!(stats.total_events_emitted, 0);
    }
}