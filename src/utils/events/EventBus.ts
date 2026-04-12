/**
 * Event bus for decoupled cross-module communication.
 * Replaces direct imports that cause circular dependencies.
 */

import type {
  CloudCoderEvent,
  EventHandler,
  EventType,
  EventBusStats,
  IEventBus,
  EventSubscription,
} from '../../types/core.js';
import type { EventBusOptions } from '../../types/events.js';

export class EventBus implements IEventBus {
  private subscribers: Map<EventType, Set<EventHandler>> = new Map();
  private allSubscribers: Set<EventHandler> = new Set();
  private eventBuffer: CloudCoderEvent[] = [];
  private stats: {
    totalEventsEmitted: number;
    eventsByType: Map<EventType, number>;
  } = {
    totalEventsEmitted: 0,
    eventsByType: new Map(),
  };
  private readonly maxBufferedEvents: number;
  private readonly debug: boolean;

  constructor(options: EventBusOptions = {}) {
    this.maxBufferedEvents = options.maxBufferedEvents ?? 100;
    this.debug = options.debug ?? false;
  }

  subscribe<T>(type: EventType, handler: EventHandler<T>): EventSubscription {
    if (!this.subscribers.has(type)) {
      this.subscribers.set(type, new Set());
    }
    this.subscribers.get(type)!.add(handler as EventHandler);

    // Deliver buffered events to new subscriber
    for (const event of this.eventBuffer) {
      if (event.type === type) {
        this.deliverEvent(handler as EventHandler, event);
      }
    }

    return {
      unsubscribe: () => {
        this.subscribers.get(type)?.delete(handler as EventHandler);
      },
    };
  }

  subscribeAll(handler: EventHandler): EventSubscription {
    this.allSubscribers.add(handler);

    // Deliver all buffered events
    for (const event of this.eventBuffer) {
      this.deliverEvent(handler, event);
    }

    return {
      unsubscribe: () => {
        this.allSubscribers.delete(handler);
      },
    };
  }

  emit<T>(event: CloudCoderEvent<T>): void {
    // Update stats
    this.stats.totalEventsEmitted++;
    const currentCount = this.stats.eventsByType.get(event.type) ?? 0;
    this.stats.eventsByType.set(event.type, currentCount + 1);

    // Buffer event
    this.eventBuffer.push(event as CloudCoderEvent);
    if (this.eventBuffer.length > this.maxBufferedEvents) {
      this.eventBuffer.shift();
    }

    // Deliver to type-specific subscribers
    const typeSubscribers = this.subscribers.get(event.type);
    if (typeSubscribers) {
      for (const handler of typeSubscribers) {
        this.deliverEvent(handler, event as CloudCoderEvent);
      }
    }

    // Deliver to catch-all subscribers
    for (const handler of this.allSubscribers) {
      this.deliverEvent(handler, event as CloudCoderEvent);
    }

    if (this.debug) {
      console.log(`[EventBus] Emitted: ${event.type}`);
    }
  }

  private async deliverEvent(handler: EventHandler, event: CloudCoderEvent): Promise<void> {
    try {
      const result = handler(event);
      if (result instanceof Promise) {
        await result.catch((err) => {
          console.error(`[EventBus] Error in event handler for ${event.type}:`, err);
        });
      }
    } catch (err) {
      console.error(`[EventBus] Error in event handler for ${event.type}:`, err);
    }
  }

  getStats(): EventBusStats {
    return {
      totalListeners: this.allSubscribers.size +
        Array.from(this.subscribers.values()).reduce((sum, set) => sum + set.size, 0),
      ...this.stats,
    };
  }

  clear(): void {
    this.subscribers.clear();
    this.allSubscribers.clear();
    this.eventBuffer = [];
    this.stats = {
      totalEventsEmitted: 0,
      eventsByType: new Map(),
    };
  }
}
